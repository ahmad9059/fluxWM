import hashlib,json,os,signal,subprocess,time
from pathlib import Path
root=Path.cwd(); out=root/'target/native-output-followup-review'; out.mkdir(); frozen=root/'target/native-output-followup-review-snapshot'; frozen.mkdir()
commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
paths=subprocess.check_output(['git','ls-files'],text=True).splitlines()
selected=[p for p in paths if p.startswith('crates/') or (p.startswith('tools/') and p.endswith('.py')) or p in ['.github/workflows/linux.yml','docs/development/native-foundation.md','docs/development/native-outputs.md','Cargo.toml','Cargo.lock','README.md','docs/development/autostart.md','docs/development/production-status-2026-10-18.md','docs/development/runtime-config.md','docs/development/runtime-findings.md','docs/10-roadmap.md']]
manifest={}
for name in selected:
    data=subprocess.check_output(['git','show',commit+':'+name]); path=frozen/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(data);manifest[name]=hashlib.sha256(data).hexdigest()
def write(name,data): (out/name).write_text(json.dumps(data,indent=2)+'\n')
upstream=Path('/home/ahmad/.cargo/git/checkouts/smithay-312425d48e59d8c8/023b78b')
primary=[(upstream, 'smithay',p) for p in ['Cargo.toml','src/backend/session/libseat.rs','src/backend/session/mod.rs','src/backend/libinput/mod.rs','src/backend/drm/device/mod.rs','src/backend/drm/device/fd.rs','src/backend/drm/device/atomic.rs','src/backend/drm/device/legacy.rs','src/backend/allocator/gbm.rs','src/backend/egl/context.rs','src/backend/egl/display.rs','src/backend/renderer/gles/mod.rs','src/utils/fd.rs']]
registry=Path('/home/ahmad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f')
primary += [(registry/'calloop-0.14.5','calloop',p) for p in ['Cargo.toml','src/sources/mod.rs','src/loop_logic.rs']]
primary += [(registry/'input-0.10.0','input',p) for p in ['Cargo.toml','src/lib.rs','src/context.rs']]
primary += [(registry/'drm-0.14.1','drm',p) for p in ['src/control/mod.rs','src/control/connector.rs','src/control/encoder.rs','src/control/property.rs']]
primary += [(upstream,'smithay',p) for p in ['src/backend/allocator/format.rs','src/backend/drm/mod.rs']]
for base,package,name in primary:
 data=(base/name).read_bytes();relative='upstream/'+package+'/'+name;path=frozen/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);manifest[relative]=hashlib.sha256(data).hexdigest()
write('input-manifest.json',manifest);write('snapshot.json',dict(commit=commit,frozen=str(frozen),files=len(manifest)))
prompt='Independently inspect the native output follow-up. Read/Grep only in this frozen snapshot; no writes, tests, commands, agents, network or outside paths. Roughly10-16 focused calls. Previous review341ebbf found no high planner defects, flagged conditional/unmeasured hardware risk that 256 format/modifier PAIRS rejects GPU initialization, and a low wrong error classification for references to empty encoder set. New source leaves numerical cap explicit/unqualified but isolates discovery failure from GPU context ownership: discovery::prepare/finish returns plan None plus failure and empty retained raw modes; create_gpu retains renderer/GBM/DRM, emits GpuReady with explicit unavailable plan then Failure; no output may use missing plan. Manual probe still fails initialization_healthy on any failure event. Plan Some(empty) means successful outputless inventory, distinct from None. Check that failures do not escape through ? and discard context unintentionally, errors remain visible and no stale output plan is published. References now yields InvalidTopology for unknown/empty set, not Capacity. Tests include257renderer pairs and context-ready+failure event contract; no real DRM ioctl adapter or hardware execution. Full topology planner/query source is in crates/flux-native/src/output.rs and system/discovery.rs. Critically new frozen upstream/smithay/src/backend/drm/mod.rs includes free planes/plane_type/plane_formats helper omitted in first review: inspect actual primary classification, CRTC filtering, format semantics and any contradictions in planner/discovery assumptions. Original pinned drm-rs and Smithay sources also present. System.rs retains pause-on-drop around initialization; no surfaces/buffers/KMS tests/commits exist, main fluxwm still nested-only, no hardware qualification or production certification. Report remaining reachable HIGH/MEDIUM defects with trigger/path, whether previous issue/classification addressed, primary-source conclusions and actual test limitations. The numerical bound is NOT claimed sufficient for target hardware and still makes output plan unavailable; do not assume any GPU counts without measured data.' 
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
