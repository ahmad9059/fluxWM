import hashlib,json,shutil,subprocess
from pathlib import Path
out=Path('docs/evidence/nested-milestone/2026-10-18-native-target-test');out.mkdir(parents=True,exist_ok=True)
origins={}
def retain(source,relative):
 source=Path(source);destination=out/relative;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,destination);origins[str(relative)]=dict(source=str(source),sha256=hashlib.sha256(source.read_bytes()).hexdigest())
for name,relative in [('native-target-final-checks','checks'),('native-target-final-mutations','mutations'),('native-target-final-readonly-audit','readonly-audit')]:
 root=Path('target')/name
 for p in root.rglob('*'):
  if p.is_file():retain(p,Path(relative)/p.relative_to(root))
for name in ['run-native-target-final-checks.py','verify-native-target-final-checks.py','run-native-target-review.py','run-native-target-followup-review.py','prove-native-target-final-controls.py','retain-native-target-evidence.py','verify-retain-native-review.py']:
 retain(Path('target')/name,Path('producer-scripts')/name)
retain('tools/check_native_probe.py',Path('harness-sources/check_native_probe.py'))
for name in ['target.rs','target/routes.rs','system/target_test.rs','system.rs','probe.rs']:
 retain(Path('crates/flux-native/src')/name,Path('producer-source')/name)
(out/'public-origins.json').write_text(json.dumps(origins,indent=2)+'\n')
(out/'source.json').write_text(json.dumps(dict(commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),scope='Compiled manual GBM/GLES/atomic TEST_ONLY adapter; no --session or native target diagnostic executed. Device-free fixtures and --check syscall audit only. New GUI runs=0, exact nested binary continuity retained.'),indent=2)+'\n')
print('retained',len(origins),'producer artifacts')
