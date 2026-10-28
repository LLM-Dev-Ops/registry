// Run only in a single-writer isolated detached worktree. No external service is provisioned.
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
const [root, parent, head, toolsRoot, target, output] = process.argv.slice(2);
if (![root, parent, head, toolsRoot, target, output].every(Boolean)) throw Error('six arguments required');
const fw = await import(path.join(toolsRoot, 'node_modules/@metaharness/flywheel/dist/index.js'));
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const source = 'crates/llm-registry-core/src/dependency.rs';
const tests = rev => git('show', `${rev}:${source}`).split('#[cfg(test)]')[1];
if (!tests(parent) || tests(parent) !== tests(head)) throw Error('test fixtures differ');
if (git('show', `${parent}:Cargo.lock`) !== git('show', `${head}:Cargo.lock`)) throw Error('dependency resolutions differ');
const env = { ...process.env, CARGO_TARGET_DIR: target, CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_PROFILE_TEST_DEBUG: '0' };
const run = (cmd, args, seconds, cwd = root) => spawnSync('timeout', ['--kill-after=5', String(seconds), cmd, ...args],
  { cwd, env, encoding: 'utf8', timeout: (seconds + 10) * 1000, maxBuffer: 4000000 });
const holdout = ['test_topological_sort_canonical_dependency_order', 'test_cycle_path_canonical_dependency_order'];
const anchors = ['test_topological_sort', 'test_topological_sort_with_cycle', 'test_no_circular_dependency', 'test_get_all_dependencies', 'test_get_dependents'];
const measurements = {};
const evaluator = async (policy, suite) => {
  const score = m => {
    const items = suite.id === 'registry-graph-anchors-v1' ? m.anchors : m.holdout;
    const count = items.filter(Boolean).length;
    return { primary: count / items.length, noopRate: 1 - count / items.length,
      costPerWin: m.elapsedSeconds / Math.max(count, 1), itemWins: items,
      regressed: !m.anchors.every(Boolean) || [m.coreExit, m.workspaceExit, m.clippyExit, m.auditExit, m.nodeBuildExit, m.nodeTestExit, m.nodeTypesExit].some(x => x !== 0) };
  };
  if (measurements[policy.revision]) return score(measurements[policy.revision]);
  if (![parent, head].includes(policy.revision)) throw Error('unexpected revision');
  git('switch', '--detach', policy.revision);
  const start = performance.now();
  const core = run('cargo', ['test', '--locked', '--offline', '-p', 'llm-registry-core', '--lib', '-j', '2'], 60);
  const coreOutput = (core.stdout || '') + (core.stderr || '');
  if (![...holdout, ...anchors].every(n => coreOutput.includes(`dependency::tests::${n} ... `))) throw Error('expected tests did not execute');
  const passed = n => coreOutput.includes(`dependency::tests::${n} ... ok`);
  const workspace = run('cargo', ['test', '--locked', '--offline', '--workspace', '-j', '2'], 180);
  const clippy = run('cargo', ['clippy', '--locked', '--offline', '-p', 'llm-registry-core', '--all-targets', '-j', '2', '--', '-D', 'warnings'], 60);
  const audit = run('cargo', ['audit', '--file', 'Cargo.lock'], 30);
  const nodeRoot = path.join(root, 'functions');
  const build = run('npm', ['run', 'build'], 30, nodeRoot);
  const node = run('npm', ['test'], 30, nodeRoot);
  const types = run('npm', ['run', 'typecheck'], 30, nodeRoot);
  measurements[policy.revision] = { holdout: holdout.map(passed), anchors: anchors.map(passed),
    coreExit: core.status, workspaceExit: workspace.status, clippyExit: clippy.status, auditExit: audit.status,
    nodeBuildExit: build.status, nodeTestExit: node.status, nodeTypesExit: types.status,
    coreOutput, workspaceOutput: (workspace.stdout || '') + (workspace.stderr || ''),
    clippyOutput: (clippy.stdout || '') + (clippy.stderr || ''), auditOutput: (audit.stdout || '') + (audit.stderr || ''),
    nodeOutput: (node.stdout || '') + (node.stderr || ''), elapsedSeconds: (performance.now() - start) / 1000 };
  return score(measurements[policy.revision]);
};
try {
  const result = await fw.runFlywheelGenerations({ rootPolicy: { revision: parent },
    proposer: async () => ({ value: head, summary: 'Canonical Rust dependency traversal', inverse: {
      path: 'git-revision', parentBytes: parent, hash: createHash('sha256').update(parent).digest('hex') } }),
    evaluator, promotionRule: fw.meetsPromotionRule, holdout: { id: 'registry-graph-order-v1', items: holdout },
    anchor: { id: 'registry-graph-anchors-v1', items: anchors }, mutationTargets: ['revision'],
    maxGenerations: 1, signer: fw.makeSigner(), dataSource: 'LIVE' });
  const replay = fw.verifyReplayBundle(result.replayBundle, { pinnedGateFingerprint: fw.gateFingerprint(fw.meetsPromotionRule) });
  const report = { parent, head, measurements, gateFingerprint: fw.gateFingerprint(fw.meetsPromotionRule),
    replay: { pass: replay.pass, chainSummary: replay.chainSummary }, finalPolicy: result.finalPolicy,
    commits: result.replayBundle.all_commits.map(c => ({ verdict: c.verdict, failureReasons: c.failureReasons, primaryDelta: c.primaryDelta })), replayBundle: result.replayBundle };
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ ...report, measurements: undefined, replayBundle: undefined }));
} finally { git('switch', '--detach', head); }
