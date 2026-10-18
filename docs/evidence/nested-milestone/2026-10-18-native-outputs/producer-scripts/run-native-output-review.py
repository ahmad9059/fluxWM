import hashlib,json,os,signal,subprocess,time
from pathlib import Path
root=Path.cwd(); out=root/'target/native-output-review'; out.mkdir(); frozen=root/'target/native-output-review-snapshot'; frozen.mkdir()
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
primary += [(upstream,'smithay','src/backend/allocator/format.rs')]
for base,package,name in primary:
 data=(base/name).read_bytes();relative='upstream/'+package+'/'+name;path=frozen/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);manifest[relative]=hashlib.sha256(data).hexdigest()
write('input-manifest.json',manifest);write('snapshot.json',dict(commit=commit,frozen=str(frozen),files=len(manifest)))
prompt='Independently review frozen optional native output discovery and candidate route planner. Read/Grep only within cwd, no commands, tests, writes, network, outside paths or agents. Roughly16-24 focused calls. Inspect crates/flux-native/src/output.rs, system/discovery.rs and system.rs integration, tests therein, docs/development/native-outputs.md, actual pinned primary APIs in upstream/drm and upstream/smithay. Main fluxwm remains nested-only. This stage adds topology/route candidates to GPU initialization, NO output surfaces, buffer allocation, KMS test/commit or hardware qualification. Manual --session is unexecuted. Planner routes connector->encoder->CRTC->primary plane with each resource capacity1 and residual augmenting paths; maximum cardinality for supplied graph, not minimum disruption nor proof of bandwidth/atomic feasibility. IDs sorted, current route preferences tie ordering; disconnected/unknown/non-desktop and no valid mode excluded. Mode ranking preferred then area then rational refresh then original index; raw kernel mode objects privately retained by connector/index, not reconstructed from rounded fields. Exact plane/render format-modifier intersections only, imports not substituted; full-width modifiers string serialized. Bounds64connectors/128encoders/32CRTC/128planes/256modes/256formats/256properties. Query coordinator fails atomically on query errors, inconsistent plane formats, mode-index mismatch, reference/count/identity problems or resource-ID churn; same IDs not claimed atomic metadata snapshot. Actual encoder masks use original ResourceHandles ordering. Shared primary planes must not be assigned twice. GPU pause-on-drop guard covers discovery errors and is disarmed only into prepared ownership; existing ticket epoch gates remain. Author evidence: exhaustive4096 two-by-two three-layer adjacency graphs matched independent brute-force route combination oracle; query fixture injects failure at every8positions; modifier/rerouting/churn negative-control mutations planned separately. You run nothing and must not assume author claims independently verified. Look for actual reachable HIGH/MEDIUM defects, invalid assumptions about pinned queries/mode/plane semantics, incorrect graph capacity/decoding/maximum matching, unsupported flags or index errors, unsafe/unbounded behavior within stated trust, new ownership bugs, tests that mirror implementation or miss real paths. Separate missing KMS/native/full production requirements explicitly documented from concrete defects. Give precise paths/lines/triggers, original findings and test limitations. No certification.' 
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
