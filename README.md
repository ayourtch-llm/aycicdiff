# aycicdiff

A Cisco IOS/IOS-XE configuration diff utility that generates incremental
configuration deltas suitable for applying via `copy file run`.

Given a running configuration and a target configuration, aycicdiff produces the
minimal set of IOS commands needed to transform one into the other -- including
proper `no` negations for removals, dependency-aware ordering, and
section-type-aware diffing.

## Why?

Manually diffing IOS configs and writing the delta is error-prone. A naive
textual diff doesn't understand IOS semantics: that removing a command requires
a `no` prefix, that ACLs must be replaced wholesale, that a route-map must exist
before a BGP neighbor references it, or that `default interface` resets routing
protocol state that needs to be re-applied.

aycicdiff understands all of this. It parses configs into a hierarchical tree,
diffs them structurally, resolves dependencies between resources, and emits
correct, ordered IOS commands.

## Quick start

```bash
# Build
cargo build --release

# Generate a config delta
aycicdiff -r running.cfg -t target.cfg

# Read running config from stdin
ssh router "show run" | aycicdiff -r - -t target.cfg

# Write delta to a file
aycicdiff -r running.cfg -t target.cfg -o delta.cfg

# Enable version-aware defaults filtering
aycicdiff -r running.cfg -t target.cfg -v show_version.txt

# Dry run with verbose output
aycicdiff -r running.cfg -t target.cfg --dry-run --verbose
```

## Example

Given a running config:

```
hostname Router1
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
interface GigabitEthernet0/1
 ip address 10.0.1.1 255.255.255.0
 shutdown
!
ip route 0.0.0.0 0.0.0.0 10.0.0.254
!
line vty 0 4
 transport input ssh
```

And a target config that changes the hostname, reconfigures GigabitEthernet0/1,
adds a static route, and adds `login local` to the VTY lines:

```
hostname Router1-NEW
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
interface GigabitEthernet0/1
 ip address 192.168.1.1 255.255.255.0
 no shutdown
!
ip route 0.0.0.0 0.0.0.0 10.0.0.254
ip route 192.168.0.0 255.255.0.0 10.0.1.254
!
line vty 0 4
 transport input ssh
 login local
```

aycicdiff generates:

```
no hostname Router1
interface GigabitEthernet0/1
 no ip address 10.0.1.1 255.255.255.0
 no shutdown
 ip address 192.168.1.1 255.255.255.0
 no shutdown
exit
ip route 192.168.0.0 255.255.0.0 10.0.1.254
line vty 0 4
 login local
exit
hostname Router1-NEW
```

## CLI reference

```
aycicdiff [OPTIONS] -r <RUNNING> -t <TARGET>
```

| Flag | Description |
|------|-------------|
| `-r, --running <PATH>` | Path to running config file, or `"-"` for stdin |
| `-t, --target <PATH>` | Path to target config file |
| `-v, --version-file <PATH>` | Path to `show version` output (enables version-aware defaults filtering) |
| `-c, --rules <PATH>` | Path to custom rules file (TOML, extends built-in rules) |
| `-o, --output <PATH>` | Write output to file instead of stdout |
| `--dry-run` | Show what would be generated without writing (implies verbose) |
| `--verbose` | Enable verbose output to stderr |
| `--dump-rules` | Print the effective rules (built-in + user) as TOML and exit |
| `--rebuild-changed-interfaces` | Reset changed physical interfaces with `default interface` before applying full target config |
| `--bounce-changed-interfaces` | Wrap changed physical interface diffs in `shutdown` / `no shutdown` |

The `--rebuild-changed-interfaces` and `--bounce-changed-interfaces` flags are
mutually exclusive.

## Interface management modes

### Default (incremental)

Only the changed commands within an interface are emitted. Removals appear as
`no` commands, additions as-is. This is the least disruptive mode.

### `--bounce-changed-interfaces`

The incremental diff for each changed physical interface is wrapped in a
`shutdown` at the top and `no shutdown` at the bottom, causing a brief link
flap during reconfiguration. Useful when the interface needs to be
re-initialized for changes to take effect. If the target state is already
`shutdown`, no wrapping is added.

```
interface GigabitEthernet0/1
 shutdown
 no ip address 10.0.1.1 255.255.255.0
 ip address 192.168.1.1 255.255.255.0
 no shutdown
exit
```

### `--rebuild-changed-interfaces`

Each changed physical interface is fully reset with `default interface X`, then
the complete target configuration is applied from scratch. This is the most
deterministic mode -- it guarantees no stale config remnants -- but is the most
disruptive.

```
default interface GigabitEthernet0/1
interface GigabitEthernet0/1
 shutdown
 ip address 192.168.1.1 255.255.255.0
 no shutdown
exit
```

When `default interface` resets implicit state (e.g., `passive-interface default`
in OSPF/EIGRP), aycicdiff automatically re-emits the necessary routing protocol
commands to restore the correct state.

## How it works

### 1. Parsing

Configs are tokenized by indentation level and built into a hierarchical
`ConfigTree`. The parser handles IOS preamble lines (`Building configuration...`,
`Current configuration`, comments), and supports both IOS classic (1-space
indent) and IOS-XE (variable indent) formats.

### 2. Section classification

Each section in the tree is classified into one of three kinds:

- **Set** (default) -- children are unordered and diffed as a set. Used for
  interfaces, router sections, VRF definitions, line configs, etc.
- **OrderedList** -- child order matters; the section is replaced wholesale if
  any entry changes. Used for ACLs, prefix-lists, community-lists, AS-path
  access-lists, and kron policy-lists.
- **Opaque** -- the entire section is treated as an indivisible blob. Used for
  banners and crypto PKI certificates.

### 3. Structural diff

The diff engine recursively compares the running and target trees:

- **Leaf nodes** are compared by full text (or by keyword prefix for
  singleton commands like `hostname`, `enable secret`, etc.).
- **Set sections** have their children diffed recursively, producing
  per-command add/remove actions.
- **OrderedList sections** are compared as a whole and replaced entirely if
  different.
- **Opaque sections** are compared as text blobs and replaced if different.

### 4. Dependency ordering

Before emitting the delta, a topological sort (Kahn's algorithm) orders actions
so that dependencies are satisfied:

- Resources are created before they're referenced (e.g., a `route-map` is
  added before a `neighbor ... route-map` statement).
- References are removed before the resources they depend on (e.g., a
  `neighbor ... route-map` is removed before the `route-map` itself).

Supported resource types: route-maps, prefix-lists, ACLs, VRFs, policy-maps,
and class-maps.

### 5. Serialization

The ordered diff actions are emitted as IOS commands:

- Removals become `no` commands (with configurable negation overrides).
- Within modified sections, removals are emitted before additions.
- Each section block is terminated with `exit`.
- Version-default commands are filtered out to avoid spurious `no` commands.

## Custom rules

aycicdiff ships with sensible built-in rules. You can extend them with a TOML
file passed via `--rules`. User rules are merged with (not replacing) the
built-in rules.

```bash
# See the full effective ruleset
aycicdiff --dump-rules

# Apply custom rules
aycicdiff -r running.cfg -t target.cfg -c my_rules.toml
```

The rules file supports the following sections:

```toml
[sections]
# Additional regex patterns for section classification
ordered = ['(?i)^ip sla\b']
opaque  = ['(?i)^macro name\b']
set     = []   # Override other classifications back to Set

[singletons]
# Commands identified by keyword prefix only (not full text)
keywords = ["ip default-network"]

[negation]
# Override negation for specific commands: [command, negated_form]
pairs = [["switchport", "no switchport"]]

[defaults]
# Commands to suppress as implicit defaults
common      = []
ios_xe      = []
ios_classic = []
```

See [`examples/rules.toml`](examples/rules.toml) for a fully commented example.

## Version-aware behavior

When a `show version` output file is provided via `--version-file`, aycicdiff
detects the platform (IOS-XE vs. IOS classic) and version, and:

- Filters out platform-specific default commands that would otherwise appear as
  spurious removals (e.g., `ip http server` on IOS-XE).
- Applies platform-specific syntax variations (e.g., `vrf forwarding` vs.
  `ip vrf forwarding`).

## Library usage

aycicdiff can be used as a Rust library:

```rust
use aycicdiff::{generate_delta, generate_delta_with_rules, DeltaOptions};
use aycicdiff::rules::RulesConfig;

// Simple API
let delta = generate_delta(running_config, target_config, None);

// With custom rules and options
let rules = RulesConfig::load_from_file("rules.toml").unwrap();
let options = DeltaOptions {
    rebuild_changed_interfaces: false,
    bounce_changed_interfaces: true,
};
let delta = generate_delta_with_rules(
    running_config,
    target_config,
    Some(show_version_output),
    &rules,
    &options,
);
```

## Project structure

```
src/
  main.rs             CLI entry point
  lib.rs              Public API
  parser/             Config text -> ConfigTree
    lexer.rs          Line tokenizer (indent, text, line number)
    tree_builder.rs   Indent-stack tree construction
  model/              Core data types
    config_tree.rs    ConfigTree, ConfigNode, ConfigSection, ConfigLeaf
    section_kind.rs   Set / OrderedList / Opaque classification
    command.rs        Parsed command (keyword, args, negated flag)
    version_info.rs   Platform and version data
  diff/               Tree comparison
    tree_diff.rs      Recursive structural diff algorithm
    diff_model.rs     DiffAction types (Add, Remove, Modify, Replace)
  serialize/           Diff -> IOS commands
    emitter.rs        Final text emission with indentation and exits
    negation.rs       Command negation rules and lookup
    dependency.rs     Topological sort for resource ordering
    bounce.rs         Interface rebuild/bounce implementations
  rules/              Extensible rule system
    config.rs         TOML-based rules (sections, singletons, negation, defaults)
  version/            Platform detection
    parser.rs         Parse "show version" output
    defaults.rs       Version-default command lists
    quirks.rs         Platform-specific syntax variations
```

## Building and testing

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug aycicdiff -r running.cfg -t target.cfg
```

## License

See [Cargo.toml](Cargo.toml) for package metadata.
