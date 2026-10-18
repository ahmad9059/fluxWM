import hashlib,json,shutil,subprocess
from pathlib import Path
out=Path('docs/evidence/nested-milestone/2026-10-18-native-outputs');out.mkdir(parents=True,exist_ok=True)
origins={}
def retain(source,relative):
 source=Path(source);destination=out/relative;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,destination);origins[str(relative)]=dict(source=str(source),sha256=hashlib.sha256(source.read_bytes()).hexdigest())
for name,relative in [('native-output-release-checks','checks'),('native-output-final-mutations','mutations'),('native-output-release-audit','readonly-audit')]:
 root=Path('target')/name
 for p in root.rglob('*'):
  if p.is_file():retain(p,Path(relative)/p.relative_to(root))
for name in ['run-native-output-release-checks.py','verify-native-output-release.py','run-native-output-review.py','run-native-output-followup-review.py','prove-native-output-final-controls.py','retain-native-output-evidence.py','verify-retain-native-review.py']:
 retain(Path('target')/name,Path('producer-scripts')/name)
retain('tools/check_native_probe.py',Path('harness-sources/check_native_probe.py'))
for name in ['output.rs','system.rs','system/discovery.rs']:
 retain(Path('crates/flux-native/src')/name,Path('producer-source')/name)
(out/'public-origins.json').write_text(json.dumps(origins,indent=2)+'\n')
(out/'source.json').write_text(json.dumps(dict(commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),scope='Native output discovery/route candidates only; no --session, native device or hardware output execution. New GUI runs=0; identical nested executables reference previous qualification.'),indent=2)+'\n')
print('retained',len(origins),'producer artifacts')
