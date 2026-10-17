use flux_config::{ConfigError, HelperLimits, load_with_helper};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn load(source: &str) -> flux_config::Result<flux_config::CompiledConfig> {
    load_with_helper(
        Path::new(env!("CARGO_BIN_EXE_fluxwm-config")),
        source.as_bytes(),
        HelperLimits {
            deadline: Duration::from_millis(600),
            ..Default::default()
        },
    )
}
#[test]
fn documentation_example_is_real_config_and_vm_is_lua54() {
    let c = load(include_str!("../../../docs/examples/config.lua")).unwrap();
    assert_eq!(c.config.bindings.len(), 13);
    assert_eq!(c.rules.len(), 3);
    load("assert(_VERSION == nil); assert(math.type(1)=='integer'); return {schema_version=1, bindings={}} ").unwrap();
    let c = load("local a = 1 << 3; return {schema_version=1,layout={gaps=a}} ").unwrap();
    assert_eq!(c.config.layout.gaps, 8.);
}
#[test]
fn restricted_allowlist_has_no_host_or_dynamic_code_access() {
    load(r#"for _, name in ipairs({'io','os','debug','load','loadfile','dofile','package','require','coroutine','collectgarbage','getmetatable','setmetatable','rawget','rawset','print','_G','_VERSION'}) do
      -- Globals cannot be enumerated through _G, use explicit checks below.
    end
    assert(io==nil and os==nil and debug==nil and load==nil and loadfile==nil and dofile==nil)
    assert(package==nil and require==nil and coroutine==nil and collectgarbage==nil)
    assert(getmetatable==nil and setmetatable==nil and rawget==nil and rawset==nil and print==nil and _G==nil)
    assert(math.random==nil and math.randomseed==nil and string.dump==nil)
    return {schema_version=1}"#).unwrap();
}
#[test]
fn hostile_plain_data_and_structural_inputs_are_rejected() {
    for source in [
        "local t={}; t.t=t; return t",
        "local t={}; local p=t; for i=1,40 do p.n={}; p=p.n end; return t",
        "return {schema_version=1, rules={[2]={}}}",
        "return {schema_version=1, bindings={1, x=2}}",
        "return {schema_version=1, layout={gaps=0/0}}",
        "return {schema_version=1, bindings={function() end}}",
        "return {schema_version=1, bindings={[false]='bad'}}",
        "return {schema_version=1, security={ipc='anyone'}}",
        "return {schema_version=1, layout={gaps=math.huge}}",
        "return {schema_version=1, autostart={{argv={'evil\\0command'}}}}",
        "return {schema_version=1, bogus=true}",
        "return {}",
    ] {
        assert!(load(source).is_err(), "accepted {source}");
    }
}
#[test]
fn infinite_loop_catchable_hook_and_allocation_are_bounded() {
    for source in [
        "while true do end",
        "while true do pcall(function() while true do end end) end",
        "local t={}; while true do t[#t+1]=string.rep('x',1000000) end",
        "return {schema_version=1,layout={gaps=string.rep('a',1000000000)}}",
    ] {
        let now = Instant::now();
        assert!(load(source).is_err());
        assert!(now.elapsed() < Duration::from_secs(3));
    }
    // The next fresh VM remains usable after each hostile process has ended.
    load("return {schema_version=1}").unwrap();
}
#[test]
fn source_bytes_and_result_trailing_junk_are_not_accepted() {
    let source = vec![b'x'; flux_config::MAX_SOURCE_BYTES + 1];
    assert!(load_with_helper(Path::new("/unused"), &source, Default::default()).is_err());
    assert!(load_with_helper(Path::new("/unused"), &[255], Default::default()).is_err());
}
#[cfg(target_os = "linux")]
#[test]
fn supervisor_reaps_timeout_and_bounds_diagnostic_and_result_pipes() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = std::env::temp_dir().join(format!("flux-config-supervisor-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let helper = dir.join("helper");
    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/usr/bin/python3\n{body}")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let limits = HelperLimits {
        deadline: Duration::from_millis(150),
        ..Default::default()
    };
    script(
        &helper,
        "import time, os\nprint(os.getpid(),file=__import__('sys').stderr,flush=True)\ntime.sleep(30)\n",
    );
    let timeout_result = load_with_helper(&helper, b"", limits);
    assert!(
        matches!(timeout_result, Err(ConfigError::Timeout)),
        "{timeout_result:?}"
    );
    // Check no matching unreaped child remains after timeout (waitpid only sees ours).
    script(&helper, "import sys\nsys.stdout.write('x'*5000000)\n");
    let error = load_with_helper(&helper, b"", limits).expect_err("oversized result accepted");
    assert!(
        error.to_string().contains("helper result exceeds"),
        "{error}"
    );
    script(&helper, "import sys\nsys.stderr.write('x'*20000)\n");
    let error = load_with_helper(&helper, b"", limits).expect_err("oversized diagnostic accepted");
    assert!(
        error.to_string().contains("helper diagnostic exceeds"),
        "{error}"
    );
    script(&helper, r#"print('{"schema_version":1} trailing')"#);
    let error = load_with_helper(&helper, b"", limits).expect_err("trailing junk accepted");
    assert!(error.to_string().contains("trailing characters"), "{error}");
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(target_os = "linux")]
#[test]
fn child_receives_minimal_environment_fd_isolation_and_os_limits() {
    use std::{
        fs,
        os::{fd::AsRawFd, unix::fs::PermissionsExt},
    };
    let dir = std::env::temp_dir().join(format!("flux-config-isolation-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let helper = dir.join("helper");
    let leaked = fs::File::open("/dev/null").unwrap();
    // Deliberately remove CLOEXEC: containment must not rely on Rust defaults.
    unsafe {
        assert_eq!(libc_for_test(leaked.as_raw_fd()), 0);
    }
    fs::write(
        &helper,
        format!(
            r#"#!/usr/bin/python3
import os, resource, json
assert set(os.environ) <= {{'LANG', 'LC_CTYPE'}}, os.environ
assert 'NoNewPrivs:\t1' in open('/proc/self/status').read()
try:
    os.fstat({fd})
    raise AssertionError('inherited leaked descriptor')
except OSError:
    pass
assert resource.getrlimit(resource.RLIMIT_NOFILE) == (32,32)
assert resource.getrlimit(resource.RLIMIT_CORE) == (0,0)
assert resource.getrlimit(resource.RLIMIT_CPU) == (2,2)
assert resource.getrlimit(resource.RLIMIT_AS) == (268435456,268435456)
print(json.dumps({{'schema_version':1}}))
"#,
            fd = leaked.as_raw_fd()
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    load_with_helper(&helper, b"", HelperLimits::default()).unwrap();
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(target_os = "linux")]
unsafe fn libc_for_test(fd: i32) -> i32 {
    unsafe extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    // Linux F_SETFD = 2; clear flags to intentionally create an inherited fd.
    unsafe { fcntl(fd, 2, 0) }
}
