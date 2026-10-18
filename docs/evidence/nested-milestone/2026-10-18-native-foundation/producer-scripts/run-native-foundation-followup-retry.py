import hashlib,json,os,signal,subprocess,time
from pathlib import Path
root=Path.cwd(); out=root/'target/native-foundation-followup-retry-review'; out.mkdir(); frozen=root/'target/native-foundation-followup-retry-snapshot'; frozen.mkdir()
commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
paths=subprocess.check_output(['git','ls-files'],text=True).splitlines()
selected=[p for p in paths if p.startswith('crates/') or (p.startswith('tools/') and p.endswith('.py')) or p in ['.github/workflows/linux.yml','docs/development/native-foundation.md','Cargo.toml','Cargo.lock','README.md','docs/development/autostart.md','docs/development/production-status-2026-10-18.md','docs/development/runtime-config.md','docs/development/runtime-findings.md','docs/10-roadmap.md']]
manifest={}
for name in selected:
    data=subprocess.check_output(['git','show',commit+':'+name]); path=frozen/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(data);manifest[name]=hashlib.sha256(data).hexdigest()
def write(name,data): (out/name).write_text(json.dumps(data,indent=2)+'\n')
upstream=Path('/home/ahmad/.cargo/git/checkouts/smithay-312425d48e59d8c8/023b78b')
primary=[(upstream, 'smithay',p) for p in ['Cargo.toml','src/backend/session/libseat.rs','src/backend/session/mod.rs','src/backend/libinput/mod.rs','src/backend/drm/device/mod.rs','src/backend/drm/device/fd.rs','src/backend/drm/device/atomic.rs','src/backend/drm/device/legacy.rs','src/backend/allocator/gbm.rs','src/backend/egl/context.rs','src/backend/egl/display.rs','src/backend/renderer/gles/mod.rs','src/utils/fd.rs']]
registry=Path('/home/ahmad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f')
primary += [(registry/'calloop-0.14.5','calloop',p) for p in ['Cargo.toml','src/sources/mod.rs','src/loop_logic.rs']]
primary += [(registry/'input-0.10.0','input',p) for p in ['Cargo.toml','src/lib.rs','src/context.rs']]
for base,package,name in primary:
 data=(base/name).read_bytes();relative='upstream/'+package+'/'+name;path=frozen/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);manifest[relative]=hashlib.sha256(data).hexdigest()
write('input-manifest.json',manifest);write('snapshot.json',dict(commit=commit,frozen=str(frozen),files=len(manifest)))
prompt='Independently review the native foundation follow-up. Read/Grep only within this frozen snapshot, no writes, commands, tests, network, agents or outside paths. Focus on crates/flux-native/src/system.rs and probe.rs, tools/check_native_probe.py, tools/test_native_probe_audit.py, docs/development/native-foundation.md and relevant pinned upstream. Prior review of 0647fb3 found M1 hotplug retirement dropped active DrmDevice causing stale KMS restore; M2 auditor missed creat, file handles, console/vendor paths and VT/vendor ioctls; L1 notification batches continued after synchronous stop. Current source uses retire_paused for all prepared GPU retirement (pause before field drop), dispatch_events stops batches when model closed, probe health requires active nonclosed and nonempty ready GPUs. Partial-init active DrmDevice drop still potentially restores KMS; explicitly documented/manual unexecuted. Audit uses PTRACE_GET_SYSCALL_INFO to reject incompatible ABI and entry/exit independent of toggles, child PDEATHSIG plus ptrace EXITKILL, creat/open/at/at2 paths, deny /dev except null zero full random urandom, filehandle calls and K/E/V/F/d ioctls, resolves child proc aliases, compares harness hash too. Owned harmless compiled fixtures exercise actual denial syscalls plus normal pass. Author reports 7 fixtures pass and native tests pass, not your executed results. Remaining Kernel/Driver/input-pin callback fault and hardware gaps explicitly documented; full native output integration out of scope. Review the fixes and new bugs, report concrete remaining HIGH/MEDIUM issues, lower issues, whether previous findings addressed and precise test limits. Inspect actual code/upstream and do not infer production or hardware certification. Roughly12-20 focused tool calls, read only.' 
(out/'prompt.txt').write_text(prompt+'\n')
argv=['claude','-p',prompt,'--output-format','json','--verbose','--tools','Read,Grep','--allowedTools','Read,Grep','--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--disable-slash-commands','--no-session-persistence','--max-turns','34','--permission-mode','plan']
write('command.json',dict(argv=argv,cwd=str(frozen)))
started=time.monotonic()
with (out/'stdout.json').open('wb') as stdout,(out/'stderr.log').open('wb') as stderr:
    p=subprocess.Popen(argv,cwd=frozen,stdout=stdout,stderr=stderr,start_new_session=True)
    write('process.json',dict(pid=p.pid,pgid=os.getpgid(p.pid)))
    try: code=p.wait(timeout=480)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid,signal.SIGTERM)
        try:code=p.wait(timeout=5)
        except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);code=p.wait()
write('execution.json',dict(exit_code=code,duration_seconds=round(time.monotonic()-started,3),snapshot_unchanged=all(hashlib.sha256((frozen/k).read_bytes()).hexdigest()==v for k,v in manifest.items())))
print(json.dumps(dict(exit_code=code,review=str(out))))
