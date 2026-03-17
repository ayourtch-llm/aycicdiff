# aycicdiff — Cisco IOS/IOS-XE Config Diff Utility — Implementation Plan

## Context

Building a Rust CLI tool that takes a device's current `show run` + `show version` output and a target configuration, then generates a config file that — when applied via `copy file run` — transforms the running config into the target. Since `copy file run` **merges** (doesn't replace), the output must include explicit `no ...` commands for removals, and must order operations to respect dependencies (e.g., create a route-map before referencing it).

---

## 1. Module & File Structure

```
src/
  main.rs                       # CLI entry (clap)
  lib.rs                        # Public API re-exports

  parser/
    mod.rs                      # parse_config() entry point
    lexer.rs                    # Line tokenizer: indent depth, bangs, comments
    tree_builder.rs             # Builds ConfigTree from tokens via indent stack
    section_classifier.rs       # Assigns SectionKind to parsed sections

  model/
    mod.rs
    config_tree.rs              # ConfigTree, ConfigNode, ConfigSection, ConfigLeaf
    section_kind.rs             # SectionKind enum (Set, OrderedList, Opaque)
    command.rs                  # Command type (keyword + args + negated flag)
    version_info.rs             # Parsed "show version" data

  diff/
    mod.rs                      # diff_configs() entry point
    tree_diff.rs                # Recursive tree differ → DiffTree
    diff_model.rs               # DiffAction enum, DiffNode, DiffTree
    ordered_diff.rs             # LCS-based diff for ordered sections (ACLs)

  serialize/
    mod.rs                      # serialize_delta() entry point
    negation.rs                 # Negation rules registry (command → "no" form)
    dependency.rs               # DAG + topological sort for ordering
    emitter.rs                  # Renders DiffTree → final config text

  version/
    mod.rs
    parser.rs                   # Parse "show version" output
    defaults.rs                 # Version-specific default commands to suppress
    quirks.rs                   # Version-specific negation/syntax quirks

tests/
  integration/
    parse_tests.rs
    diff_tests.rs
    serialize_tests.rs
    end_to_end.rs
  fixtures/
    simple_running.cfg
    simple_target.cfg
    acl_running.cfg
    acl_target.cfg
    full_router.cfg
    show_version_isr4k.txt
    show_version_cat9k.txt
```

---

## 2. Core Data Types

### 2.1 Config Tree (`model/config_tree.rs`)

```rust
pub struct ConfigTree {
    pub nodes: Vec<ConfigNode>,
}

pub enum ConfigNode {
    Leaf(ConfigLeaf),
    Section(ConfigSection),
}

pub struct ConfigLeaf {
    pub text: String,
    pub command: Command,
}

pub struct ConfigSection {
    pub header: String,
    pub command: Command,
    pub kind: SectionKind,
    pub children: Vec<ConfigNode>,
}

pub struct Command {
    pub keyword: String,
    pub args: Vec<String>,
    pub negated: bool,
}
```

### 2.2 Section Kind (`model/section_kind.rs`)

```rust
pub enum SectionKind {
    Set,          // Unordered, idempotent (interface, router body)
    OrderedList,  // Order matters (ACLs, prefix-lists)
    Opaque,       // Treated as blob (banners, crypto certs)
}
```

**Classification table:**

| Header pattern | Kind | Notes |
|---|---|---|
| `interface *` | Set | |
| `router ospf/bgp/eigrp *` | Set | May contain ordered sub-sections |
| `ip access-list *` | OrderedList | |
| `ip prefix-list *` | OrderedList | |
| `route-map * permit/deny *` | Set | Set of clauses is ordered by seq |
| `line *` | Set | |
| `banner *` | Opaque | Delimited block |
| `crypto pki certificate *` | Opaque | |
| Default | Set | Safe fallback |

### 2.3 Diff Model (`diff/diff_model.rs`)

```rust
pub struct DiffTree {
    pub actions: Vec<DiffAction>,
}

pub enum DiffAction {
    Add(ConfigNode),
    Remove(ConfigNode),
    ModifySection {
        header: String,
        kind: SectionKind,
        child_actions: Vec<DiffAction>,
    },
    ReplaceOrdered {
        header: String,
        remove_children: Vec<ConfigLeaf>,
        add_children: Vec<ConfigLeaf>,
    },
}
```

### 2.4 Version Info (`model/version_info.rs`)

```rust
pub struct VersionInfo {
    pub platform: Platform,     // IosClassic, IosXe, Unknown
    pub major_version: u32,
    pub minor_version: u32,
    pub train: String,          // e.g., "17.06.03a"
    pub image: String,          // e.g., "C9300-universalk9"
    pub model: String,          // e.g., "C9300-48P"
}
```

---

## 3. Key Algorithms

### 3.1 Parser

**Lexer:** Line-based tokenizer producing `Token { indent, text, line_no }`. Skips preamble (`Building configuration...`, `Current configuration :`), empty lines, comment-only `!` lines. `end` at indent 0 terminates.

**Tree builder:** Indent-stack algorithm:

```
stack = [(indent=-1, node=root)]
for each token:
  while stack.top().indent >= token.indent:
    pop and attach to new top as child
  if next_token.indent > token.indent:
    push Section(header=token.text)
  else:
    attach Leaf(token.text) to stack.top()
```

**Special cases:**
- **Banners:** Detect delimiter char, collect until closing delimiter → Opaque
- **Crypto certs:** Multi-line hex block
- **`address-family`** inside `router bgp`: nested sub-section (IOS-XE uses indentation consistently, so indent-based parser handles it)

### 3.2 Tree Diff

Recursive algorithm:

1. Build identity maps for current and target nodes
2. For each target node not in current → `Add`
3. For each current node not in target → `Remove`
4. For matching sections:
   - `OrderedList` → `ReplaceOrdered` if contents differ
   - `Set` → recurse on children → `ModifySection` if non-empty diff

**Node identity:**
- Leaf: full text (overridden for singletons like `hostname` where identity = keyword only)
- Section: full header text

### 3.3 Serializer

**Negation (`negation.rs`):**
- Default: `no <full-command-text>`
- Special cases: `shutdown` ↔ `no shutdown`, `no ip domain-lookup` → `ip domain-lookup`
- Registry of overrides

**Dependency ordering (`dependency.rs`):**
- Extract "provides" and "requires" from each DiffAction
- Build DAG, topological sort (Kahn's algorithm)
- Rules:
  - Object creation before reference (route-map before `neighbor ... route-map`)
  - Reference removal before object deletion
  - VRF definition before interface VRF assignment
  - class-map → policy-map → service-policy

**Emitter (`emitter.rs`):**
- `Add(Section)` → header + indented children
- `Remove(Section)` → `no <header>`
- `ModifySection` → header + child removes + child adds + `exit`
- `Add(Leaf)` → text
- `Remove(Leaf)` → `no <text>`
- Ordered lists: remove entire list (`no ip access-list ...`), re-add complete target list

---

## 4. Dependency Handling Detail

**Phase A — Extract objects and references:**

| Pattern | Provides/Requires |
|---|---|
| `route-map NAME` | provides `route-map:NAME` |
| `ip prefix-list NAME` | provides `prefix-list:NAME` |
| `ip access-list ... NAME` | provides `acl:NAME` |
| `vrf definition NAME` | provides `vrf:NAME` |
| `policy-map NAME` | provides `policy-map:NAME` |
| `class-map NAME` | provides `class-map:NAME` |
| `match ip address prefix-list X` | requires `prefix-list:X` |
| `match ip address X` | requires `acl:X` |
| `ip vrf forwarding X` | requires `vrf:X` |
| `service-policy ... X` | requires `policy-map:X` |
| `neighbor ... route-map X` | requires `route-map:X` |

**Phase B — Build edges:**
- For `Add` actions: if A provides X and B requires X → A before B
- For `Remove` actions: reverse — remove references before definitions

---

## 5. Phased Implementation

### Phase 1: Config Parser
- **Files:** `parser/*`, `model/config_tree.rs`, `model/section_kind.rs`, `model/command.rs`, `lib.rs`
- **Deliverables:** `parse_config(&str) -> Result<ConfigTree>`, handles all major section types
- **Tests:** Round-trip parse→serialize, fixture-based

### Phase 2: Tree Diff
- **Files:** `diff/*`
- **Deliverables:** `diff_configs(&ConfigTree, &ConfigTree) -> DiffTree`
- **Tests:** Add-only, remove-only, mixed, ACL reorder, no-change

### Phase 3: Delta Serializer
- **Files:** `serialize/*`
- **Deliverables:** `serialize_delta(&DiffTree) -> String` with negation + dependency ordering
- **Tests:** End-to-end: parse two configs → diff → serialize → verify output

### Phase 4: Version-Aware Model
- **Files:** `version/*`, update `main.rs`
- **Deliverables:** Parse `show version`, suppress version-default commands from diff, version-specific quirks

### Phase 5: CLI Polish
- `clap`-based CLI with file inputs, stdin support, `--dry-run`, `--verbose`
- Error reporting with line numbers and section context

---

## 6. IOS Sections Needing Special Handling

| Section/Command | Handling |
|---|---|
| `banner motd/login/exec` | Opaque, delimiter detection |
| `ip access-list extended/standard` | OrderedList, replace-wholesale |
| `ip prefix-list` | OrderedList, seq numbers |
| `route-map NAME permit/deny SEQ` | Each clause is a section; set of clauses ordered by seq |
| `crypto pki certificate` | Opaque hex block |
| `address-family` under `router bgp` | Nested sub-section |
| `vrf definition` / `ip vrf` | Dependency for interfaces |
| `class-map` / `policy-map` | Dependency chain |
| `line con/vty/aux` | Set section |
| `no X` in running config | Removal means removing the `no` (e.g., `no ip domain-lookup` → `ip domain-lookup`) |

---

## 7. Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
regex = "1"
thiserror = "2"
log = "0.4"
env_logger = "0.11"
similar = "2"          # LCS-based diffing for ordered sections

[dev-dependencies]
pretty_assertions = "1"
indoc = "2"
```

---

## 8. Key Design Decisions

1. **Ordered lists: replace wholesale** (delete + re-add) rather than incremental edit. Safer, simpler, matches industry tooling (NAPALM/Nornir). Brief traffic disruption acceptable for `copy file run` workflow.

2. **Command identity: full text** for leaves, with singleton override registry (`hostname`, `enable secret`, etc. where identity = keyword only).

3. **Always emit `exit`** after section modification blocks for safety with `copy file run`.

4. **Indent detection:** Count leading spaces, track increases. Works for both IOS classic (1-space) and IOS-XE address-family (2-space).

---

## 9. Verification

- **Unit tests:** Each module has tests for its core logic
- **Integration tests:** End-to-end with fixture configs
- **Manual validation:** Compare output against manually crafted deltas for real device configs
- **Round-trip property:** `parse(serialize(parse(input))) == parse(input)`
- **Diff completeness:** Applying generated delta to running config should yield target config
