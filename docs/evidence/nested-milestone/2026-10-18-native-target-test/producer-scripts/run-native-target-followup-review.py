import hashlib,json,os,signal,subprocess,time
from pathlib import Path
root=Path.cwd(); out=root/'target/native-target-followup-review'; out.mkdir(); frozen=root/'target/native-target-followup-review-snapshot'; frozen.mkdir()
commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
paths=subprocess.check_output(['git','ls-files'],text=True).splitlines()
selected=[p for p in paths if p.startswith('crates/') or (p.startswith('tools/') and p.endswith('.py')) or p in ['.github/workflows/linux.yml','docs/development/native-foundation.md','docs/development/native-outputs.md','docs/development/native-target-test.md','Cargo.toml','Cargo.lock','README.md','docs/development/autostart.md','docs/development/production-status-2026-10-18.md','docs/development/runtime-config.md','docs/development/runtime-findings.md','docs/10-roadmap.md']]
manifest={}
for name in selected:
    data=subprocess.check_output(['git','show',commit+':'+name]); path=frozen/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(data);manifest[name]=hashlib.sha256(data).hexdigest()
def write(name,data): (out/name).write_text(json.dumps(data,indent=2)+'\n')
upstream=Path('/home/ahmad/.cargo/git/checkouts/smithay-312425d48e59d8c8/023b78b')
primary=[(upstream, 'smithay',p) for p in ['Cargo.toml','src/backend/session/libseat.rs','src/backend/session/mod.rs','src/backend/libinput/mod.rs','src/backend/drm/device/mod.rs','src/backend/drm/device/fd.rs','src/backend/drm/device/atomic.rs','src/backend/drm/device/legacy.rs','src/backend/allocator/gbm.rs','src/backend/egl/context.rs','src/backend/egl/display.rs','src/backend/renderer/gles/mod.rs','src/utils/fd.rs']]
registry=Path('/home/ahmad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f')
primary += [(registry/'calloop-0.14.5','calloop',p) for p in ['Cargo.toml','src/sources/mod.rs','src/loop_logic.rs']]
primary += [(registry/'input-0.10.0','input',p) for p in ['Cargo.toml','src/lib.rs','src/context.rs']]
primary += [(registry/'drm-0.14.1','drm',p) for p in ['src/control/mod.rs','src/control/atomic.rs','src/control/connector.rs','src/control/encoder.rs','src/control/property.rs','src/control/plane.rs']]
primary += [(upstream,'smithay',p) for p in ['src/backend/allocator/format.rs','src/backend/drm/mod.rs']]
primary += [(upstream,'smithay',p) for p in ['src/backend/renderer/mod.rs','src/backend/renderer/sync/mod.rs','src/backend/renderer/sync/egl.rs','src/backend/drm/surface/mod.rs','src/backend/drm/surface/atomic.rs','src/backend/drm/surface/legacy.rs','src/backend/drm/gbm.rs','src/backend/allocator/mod.rs','src/backend/allocator/gbm.rs','src/backend/allocator/dmabuf.rs']]
for base,package,name in primary:
 data=(base/name).read_bytes();relative='upstream/'+package+'/'+name;path=frozen/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);manifest[relative]=hashlib.sha256(data).hexdigest()
write('input-manifest.json',manifest);write('snapshot.json',dict(commit=commit,frozen=str(frozen),files=len(manifest)))
prompt='Independently review frozen native target diagnostic follow-up c2e86ed. Read/Grep only within snapshot, no writes, tests, commands, network, outside paths or agents. About12-20 calls. Prior review abadd20 found M1 GLES cached imports stayed pinned after target return; M2 existing cloned connectors or moved resources changed meaning of successful hypothetical atomic test. Current code: target::run calls Backend.finish_cleanup only AFTER targets/local resource closure drops on success or failure if attempts>0; actual Adapter calls Renderer::cleanup_texture_cache; error adds cleanup failure, invalidates ok and clears tested metadata, while earlier primary error is preserved. Tests mock cache lifetime/errors, not actual GLES. Note pinned GlesFrame::finish_internal also calls cleanup during finish, but current live target dmabufs make entries survive then; final post-retirement cleanup is the actual fix. M2 new target/routes.rs pure shared reconciler checks bounded unique source snapshot, desired selected routes, detaches unplanned clones on tested CRTCs, emits ALL known connector routes (unrelated unchanged captured values), disables source CRTCs that lose final connector and clears non-selected planes on tested/disabled CRTCs. Selected primary moved from unrelated surviving-output CRTC is rejected TopologyConflict, no optimizer/backtracking claimed. Actual Adapter queries resources/all connectors CRCT_ID/all plane current routes, calls reconciler, constructs request accordingly then fixed TEST_ONLY|ALLOW_MODESET; no DrmSurface or live commit. It applies fresh connector route/property map and checks non-desktop again; disabled source MODE_ID=0 ACTIVE=0, clearing FB_ID=0 CRTC_ID=0. Test freshness still only request-time snapshot, not hardware reservation. Check whether medium findings fixed and any new reachable HIGH/MEDIUM errors, stale resource reuse/cleanup/lifetime/order, route reconciliation logical defects or false-success oracle, missing property requests, flags or unbounded application work. Files src/target.rs, target/routes.rs, system/target_test.rs, Driver test_output_targets and probe health as needed. All pinned upstream primary files from previous review are provided. Manual --session --test-outputs remains UNEXECUTED. Shared coordinator/route/flags/parser/health fixtures are author evidence only; real property/framebuffer/blob/GBM/EGL/atomic/fence/provider paths unrun. Nominal pixels not actual GPU-memory/RSS bound. Original low local-drop-order/implicit allocation false negatives/multi-GPU strictness now documented. No native output presentation, page-flips, live commit, safe-preview or hardware/production qualification. Report concrete findings with paths/triggers, checked fixes, remaining lows/test gaps; no certification.' 
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
