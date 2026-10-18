import hashlib,json,subprocess
from pathlib import Path
out=Path('target/native-output-final-mutations');out.mkdir()
def run(name,path,old,new,test):
 directory=out/name;directory.mkdir();p=Path(path);original=p.read_bytes();text=original.decode();assert text.count(old)==1
 mutated=text.replace(old,new).encode();(directory/'original.rs').write_bytes(original);(directory/'mutated.rs').write_bytes(mutated)
 command=['cargo','test','-p','flux-native','--features','system','--locked',test,'--','--nocapture']
 try:
  p.write_bytes(mutated)
  with (directory/'console.log').open('wb') as stream:completed=subprocess.run(command,stdout=stream,stderr=subprocess.STDOUT,timeout=120)
 finally:p.write_bytes(original)
 log=(directory/'console.log').read_text();assert completed.returncode==101 and 'test result: FAILED' in log and test in log
 result=dict(command=command,exit_code=completed.returncode,original_sha256=hashlib.sha256(original).hexdigest(),mutated_sha256=hashlib.sha256(mutated).hexdigest(),source_restored=p.read_bytes()==original)
 (directory/'result.json').write_text(json.dumps(result,indent=2)+'\n');return result
rows=[run('residual-rerouting','crates/flux-native/src/output.rs','if edge.available && parent[edge.to].is_none()','if edge.available && edge.forward && parent[edge.to].is_none()','exhaustive_two_by_two_resource_graphs_match_independent_oracle'),run('modifier-intersection','crates/flux-native/src/output.rs','.intersection(&render)','.union(&render)','exact_modifier_intersection_is_required_and_serializes_losslessly'),run('resource-churn','crates/flux-native/src/system/discovery.rs','if !same_resources(&resources, &after)','if false && !same_resources(&resources, &after)','detects_resource_churn_but_accepts_resource_reordering')]
(out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(dict(rejected_mutations=len(rows),sources_restored=all(row['source_restored'] for row in rows))))
