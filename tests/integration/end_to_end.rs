use aycicdiff::generate_delta;

#[test]
fn test_simple_diff() {
    let running = include_str!("../fixtures/simple_running.cfg");
    let target = include_str!("../fixtures/simple_target.cfg");

    let delta = generate_delta(running, target, None);

    // Should change hostname
    assert!(delta.contains("hostname Router1-NEW"), "Should set new hostname. Got:\n{}", delta);

    // Should modify Gi0/1: change IP and enable
    assert!(delta.contains("interface GigabitEthernet0/1"), "Should modify Gi0/1. Got:\n{}", delta);
    assert!(delta.contains("ip address 192.168.1.1 255.255.255.0"), "Should set new IP. Got:\n{}", delta);
    assert!(delta.contains("no shutdown"), "Should enable Gi0/1. Got:\n{}", delta);

    // Should add new static route
    assert!(delta.contains("ip route 192.168.0.0 255.255.0.0 10.0.1.254"), "Should add static route. Got:\n{}", delta);

    // Should add login local to vty
    assert!(delta.contains("login local"), "Should add login local. Got:\n{}", delta);

    // Should NOT contain unchanged commands
    assert!(!delta.contains("ip route 0.0.0.0 0.0.0.0 10.0.0.254"), "Should not re-add existing route. Got:\n{}", delta);
}

#[test]
fn test_acl_diff() {
    let running = include_str!("../fixtures/acl_running.cfg");
    let target = include_str!("../fixtures/acl_target.cfg");

    let delta = generate_delta(running, target, None);

    // ACL should be replaced wholesale
    assert!(delta.contains("no ip access-list extended OUTSIDE_IN"), "Should remove old ACL. Got:\n{}", delta);
    assert!(delta.contains("ip access-list extended OUTSIDE_IN"), "Should re-add ACL. Got:\n{}", delta);
    assert!(delta.contains("permit tcp any host 10.0.0.1 eq 8080"), "Should have new ACE. Got:\n{}", delta);
    assert!(delta.contains("permit tcp any host 10.0.0.2 eq 443"), "Should have new ACE. Got:\n{}", delta);
}

#[test]
fn test_no_changes() {
    let config = "\
hostname Router1
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
";
    let delta = generate_delta(config, config, None);
    assert!(delta.is_empty(), "No changes expected but got:\n{}", delta);
}

#[test]
fn test_add_new_section() {
    let running = "hostname Router1\n";
    let target = "\
hostname Router1
interface Loopback0
 ip address 1.1.1.1 255.255.255.255
";
    let delta = generate_delta(running, target, None);
    assert!(delta.contains("interface Loopback0"), "Should add Loopback0. Got:\n{}", delta);
    assert!(delta.contains("ip address 1.1.1.1 255.255.255.255"), "Should have IP. Got:\n{}", delta);
    assert!(delta.contains("exit"), "Should have exit. Got:\n{}", delta);
}

#[test]
fn test_remove_section() {
    let running = "\
hostname Router1
interface Loopback0
 ip address 1.1.1.1 255.255.255.255
";
    let target = "hostname Router1\n";
    let delta = generate_delta(running, target, None);
    assert!(delta.contains("no interface Loopback0"), "Should remove Loopback0. Got:\n{}", delta);
}

#[test]
fn test_exit_after_section_modify() {
    let running = "\
interface GigabitEthernet0/0
 shutdown
";
    let target = "\
interface GigabitEthernet0/0
 no shutdown
";
    let delta = generate_delta(running, target, None);
    assert!(delta.contains("exit"), "Should emit exit after section modification. Got:\n{}", delta);
}
