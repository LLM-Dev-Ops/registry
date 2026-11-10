const fs=require('node:fs'),cp=require('node:child_process'),path=require('node:path'),crypto=require('node:crypto');
const {pathToFileURL}=require('node:url');
async function main(){
 const [base,head,tools,out]=process.argv.slice(2).map(p=>path.resolve(p));
 const fw=await import(pathToFileURL(path.join(tools,'node_modules/@metaharness/flywheel/dist/index.js')));
 const rev=r=>cp.execFileSync('git',['rev-parse','HEAD'],{cwd:r,encoding:'utf8'}).trim(),hash=s=>crypto.createHash('sha256').update(s).digest('hex');
 const parent=rev(base),candidate=rev(head),probeFile='functions/tests/routing-http.cjs';
 for(const f of [probeFile,'docs/metaharness/2026-11-10/contract.json'])if(!fs.readFileSync(path.join(base,f)).equals(fs.readFileSync(path.join(head,f))))throw Error('Frozen probe changed');
 const probe=root=>{const r=cp.spawnSync(process.execPath,[path.join(root,probeFile),path.join(root,'functions')],{encoding:'utf8',timeout:10000});if(![0,1].includes(r.status))throw Error(r.stderr);return JSON.parse(r.stdout)};
 const trials=[];for(let i=0;i<3;i++)for(const root of i%2?[head,base]:[base,head])trials.push({i,revision:rev(root),...probe(root)});
 const tests={};for(const root of [base,head]){const r=cp.spawnSync('npm',['test'],{cwd:path.join(root,'functions'),encoding:'utf8',timeout:20000});tests[rev(root)]={exit:r.status,stdout:r.stdout,stderr:r.stderr};}
 const inverse=['functions/src/index.ts','functions/dist/index.js','functions/dist/index.js.map','functions/dist/index.d.ts.map','functions/package.json','.github/workflows/registry-agents.yml'].map(p=>{const bytes=fs.readFileSync(path.join(base,p),'utf8');return {path:p,parentBytes:bytes,sha256:hash(bytes)}});
 const measurements=[];
 const result=await fw.runFlywheelGenerations({rootPolicy:{revision:parent},proposer:async()=>({value:candidate,summary:'Host-independent native handler pathname routing',inverse}),evaluator:async(policy,suite)=>{const r=probe(policy.revision===parent?base:head),wins=suite.id==='holdout'?r.holdout:r.anchors;const score={primary:wins.filter(Boolean).length/wins.length,itemWins:wins,noopRate:0,costPerWin:0,regressed:true};measurements.push({revision:policy.revision,suite:suite.id,report:r,score});return score;},holdout:{id:'holdout',items:[0,1,2,3,4,5]},anchor:{id:'anchors',items:[0,1,2,3,4,5]},maxGenerations:1,signer:fw.makeSigner(),promotionRule:fw.meetsPromotionRule,dataSource:'LIVE_NATIVE_HTTP'});
 const gate=fw.gateFingerprint(fw.meetsPromotionRule),replay=fw.verifyReplayBundle(result.replayBundle,{pinnedGateFingerprint:gate});
 if(!replay.pass||result.promotions.length)throw Error('Unexpected promotion or invalid replay');
 fs.writeFileSync(out,JSON.stringify({parent,candidate,trials,tests,inverse,measurements,result,replay,gate,boundary:'regressed=true explicitly vetoes unavailable whole Rust/Ruflo gates and failed root locked install; not a new Node regression. cost/noop0 neutral/unmeasured. Adapter599 is test exception sentinel, not production response. Native fallback scope, not normalized Functions Framework req.path. Root-only replay is not candidate promotion.'},null,2));
 console.log(JSON.stringify({parent,candidate,promotions:result.promotions.length,replay,tests:Object.fromEntries(Object.entries(tests).map(([k,v])=>[k,v.exit]))}));
}
main().catch(e=>{console.error(e);process.exitCode=1});
