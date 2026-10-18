import hashlib,json,shutil,subprocess
from pathlib import Path
root=Path.cwd();out=root/'docs/evidence/nested-milestone/2026-10-18-native-foundation';out.mkdir(parents=True,exist_ok=True)
origins={}
def retain(src,rel):
 src=Path(src);dst=out/rel;dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst);origins[str(rel)]={'source':str(src.relative_to(root) if src.is_absolute() else src),'sha256':hashlib.sha256(src.read_bytes()).hexdigest()}
for dirname,relative in [('native-foundation-qualified-checks','checks'),('native-probe-qualified-audit','readonly-audit'),('native-control-mutations','mutations'),('native-auditor-creation-fixtures-v2','auditor-fixtures')]:
 for p in (root/'target'/dirname).rglob('*'):
  if p.is_file() and p.suffix in {'.json','.log','.rs','.c'}:retain(p,Path(relative)/p.relative_to(root/'target'/dirname))
for p in (root/'target/native-foundation-qualified-gui').rglob('*'):
 if p.is_file() and p.suffix in {'.json','.log','.stdout','.stderr','.lua','.py'}:
  retain(p,Path('runtime')/p.relative_to(root/'target/native-foundation-qualified-gui'))
for name in ['run-native-foundation-checks.py','run-native-foundation-qualified-checks.py','run-native-foundation-qualified-gui.py','run-native-foundation-followup-retry.py','verify-retain-native-review.py','run-native-foundation-qualified-checks.py','run-native-foundation-qualified-gui.py','run-native-foundation-review.py','run-native-foundation-followup.py','prove-native-controls.py','retain-native-foundation-evidence.py']:
 retain(root/'target'/name,Path('producer-scripts')/name)
for p in (root/'tools').glob('*.py'):retain(p,Path('harness-sources')/p.name)
retain(root/'target/native-build-environment.json',Path('build-environment.json'))
(out/'public-origins.json').write_text(json.dumps(origins,indent=2)+'\n')
(out/'source.json').write_text(json.dumps(dict(commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),scope='Optional native foundation; no real seat/GPU/input session or output presentation executed. Source mutations predate follow-up fixes; exact original/mutated bytes retained. Final binaries built after all cargo checks before GUI tests.',excluded='ELF executables, Xauthority cookies, screenshots, GPG keyrings, full environment and native hardware inventory paths are not retained.'),indent=2)+'\n')
print('retained',len(origins),'producer artifacts')
