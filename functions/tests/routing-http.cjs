const http = require('node:http');
const path = require('node:path');
const {once} = require('node:events');
async function evaluate(root) {
  const {handler} = require(path.resolve(root, 'dist/index.js'));
  let exceptions = 0;
  // Native HTTP adapter supplies the parsed body that Functions Framework provides.
  // All route/schema/agent decisions execute the real compiled handler.
  const server = http.createServer(async (req, res) => {
    let body = '';
    for await (const chunk of req) body += chunk;
    req.body = body ? JSON.parse(body) : undefined;
    try { await handler(req, res); }
    catch { exceptions++; res.writeHead(599); res.end('handler exception'); }
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const port = server.address().port;
  const request = (url, body, host = 'localhost') => new Promise((resolve, reject) => {
    const text = body === undefined ? undefined : JSON.stringify(body);
    const req = http.request({host:'127.0.0.1',port,path:url,method:text===undefined?'GET':'POST',headers:{host,...(text?{'content-type':'application/json','content-length':Buffer.byteLength(text)}:{})}}, res => {
      let data='';res.on('data',c=>data+=c);res.on('end',()=>resolve({status:res.statusCode,data}));
    }); req.on('error',reject); req.setTimeout(3000,()=>req.destroy(new Error('probe timeout')));req.end(text);
  });
  try {
    const statuses=[];
    for(const host of ['[','http://','%','a:b:c','bad host','@']) statuses.push((await request('/health',undefined,host)).status);
    const anchors=[(await request('/health')).status===200,(await request('/contracts')).status===200,(await request('/v1/registry/index',{mode:'full'})).status===200,(await request('/v1/registry/reputation',{agent_id:'a',operation:'query'})).status===200,(await request('/v1/registry/index',{mode:'invalid'})).status===400,(await request('/unknown')).status===404];
    return {holdout:statuses.map(s=>s===200),anchors,statuses,exceptions};
  } finally { await new Promise(resolve=>server.close(resolve)); }
}
module.exports={evaluate};
if(require.main===module)evaluate(path.resolve(process.argv[2]||'.')).then(r=>{console.log(JSON.stringify(r));if([...r.holdout,...r.anchors].some(x=>!x))process.exitCode=1;}).catch(e=>{console.error(e);process.exitCode=1;});
