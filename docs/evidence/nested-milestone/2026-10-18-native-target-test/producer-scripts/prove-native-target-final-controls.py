import hashlib,json,subprocess
from pathlib import Path
out=Path('target/native-target-final-mutations');out.mkdir()
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
rows=[run('early-target-release','crates/flux-native/src/target.rs','backend.test_only(&targets)?;','targets.clear(); backend.test_only(&targets)?;','all_targets_live_during_joint_test_and_retire_before_return'),run('cleanup-accounting','crates/flux-native/src/target.rs','if failures > 0 && report.failure.is_none()','if false && failures > 0 && report.failure.is_none()','cleanup_failure_cannot_be_reported_as_a_successful_test'),run('test-only-flag','crates/flux-native/src/system/target_test.rs','fn flags() -> AtomicCommitFlags {\n    AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET\n}','fn flags() -> AtomicCommitFlags {\n    AtomicCommitFlags::ALLOW_MODESET\n}','kernel_request_flags_always_test_only_without_events_or_nonblock')]
rows += [run('renderer-cache-retention','crates/flux-native/src/target.rs','let final_cleanup = if report.attempts > 0','let final_cleanup = if false && report.attempts > 0','renderer_cache_retires_after_targets_and_errors_cannot_pass'),run('cloned-connector-retention','crates/flux-native/src/target/routes.rs','if !planned.contains(connector) && crtc.is_some_and','if false && !planned.contains(connector) && crtc.is_some_and','detaches_unplanned_clones_and_preserves_unrelated_routes')]

(out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(dict(rejected_mutations=len(rows),sources_restored=all(row['source_restored'] for row in rows))))
