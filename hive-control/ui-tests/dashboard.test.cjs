const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {test} = require('node:test');
const {JSDOM, VirtualConsole} = require('jsdom');
const html = fs.readFileSync(path.join(__dirname,'../src/dashboard.html'),'utf8');
const tick = () => new Promise(resolve => setTimeout(resolve,20));
function fixture(t, options = {}) {
  const errors = [], requests = [];
  const state = {down:false, postStatus:200, held:null, wrongAgent:false, malformed:false, timeout:options.timeout,
    nodes:[
      {info:{id:'thinkcentre-llm',hostname:'THINKCENTRE',cpu:'Intel Core i5-7400T',cores:4,memory_mb:8192,architecture:'x86_64',capabilities:['Inference']},status:'Online',cpu_usage:28.4,memory_used_mb:6042,last_seen_seconds_ago:1},
      {info:{id:'field/node 01',hostname:'FIELD',cpu:'Sample CPU',cores:2,memory_mb:4096,architecture:'x86_64',capabilities:['Tools','Storage']},status:'Online',cpu_usage:4.8,memory_used_mb:500,last_seen_seconds_ago:2},
      {info:{id:'<img src=x onerror=alert(1)>',hostname:'OFFLINE',cpu:'Sample CPU',cores:2,memory_mb:4096,architecture:'x86_64',capabilities:['Tools']},status:'Offline',cpu_usage:90,memory_used_mb:3500,last_seen_seconds_ago:47}
    ]};
  const virtualConsole = new VirtualConsole();
  virtualConsole.on('jsdomError',error => errors.push(error.message));
  const dom = new JSDOM(html,{url:'http://hive.test:8080/',runScripts:'dangerously',pretendToBeVisual:true,virtualConsole,beforeParse(window) {
    if (options.history) window.sessionStorage.setItem('fieldhive.activity',JSON.stringify(options.history));
    if (options.timeout) {
      const setTimeout = window.setTimeout.bind(window);
      window.setTimeout = (callback,delay,...args) => setTimeout(callback,delay === 200000 ? 2 : delay,...args);
    }
    window.fetch = async (url,init={}) => {
      if (url === '/nodes') {
        if (state.down) throw new Error('Connection refused');
        return {ok:true,json:async () => state.malformed ? {} : structuredClone(state.nodes)};
      }
      const data = JSON.parse(init.body); requests.push({url,data});
      if (state.timeout) await new Promise((_,reject) => init.signal.addEventListener('abort',() => reject(Object.assign(new Error('Aborted'),{name:'AbortError'})),{once:true}));
      if (state.held) await state.held;
      const nodeId = decodeURIComponent(url.split('/')[2]);
      let output = 'Model answer from ' + nodeId;
      if (data.task.type === 'echo') output = data.task.message;
      if (data.task.type === 'get_system_info') output = 'Node ' + nodeId + ' is online';
      if (data.task.type === 'store_observation') output = 'Observation stored';
      if (data.task.type === 'list_observations') output = JSON.stringify([{text:'Fresh deer tracks.',timestamp:1792290000},{text:'<script>alert(1)</script>',timestamp:1e30}]);
      return {ok:state.postStatus===200,status:state.postStatus,json:async () => ({node_id:state.wrongAgent ? 'different-agent' : nodeId,output})};
    };
  }});
  const w = dom.window, d = w.document;
  const q = selector => d.querySelector(selector);
  const el = id => d.getElementById('hp-' + id);
  const input = text => {el('input').value = text; el('input').dispatchEvent(new w.Event('input'));};
  const select = id => Array.from(d.querySelectorAll('[data-node-id]')).find(node => node.dataset.nodeId === id).click();
  const action = name => q('[data-action="' + name + '"]').click();
  const submit = async () => {el('form').dispatchEvent(new w.Event('submit',{cancelable:true})); await tick();};
  const refresh = async () => {el('refresh').click(); await tick();};
  t.after(() => {dom.window.close(); assert.deepEqual(errors,[],'No uncaught UI errors');});
  return {w,d,q,el,input,select,action,submit,refresh,state,requests};
}

test('live fleet selects inference and safely renders node identifiers',async t => {
  const ui = fixture(t); await tick();
  assert.equal(ui.el('online-count').textContent,'02');
  assert.equal(ui.el('registered-count').textContent,' / 03');
  assert.equal(ui.el('selected-name').textContent,'thinkcentre-llm');
  assert.equal(ui.el('controller-mode').textContent,'LINK ACTIVE');
  assert.equal(ui.d.querySelectorAll('img').length,0);
  assert.equal(ui.el('run').disabled,true);
  ui.input('Explain networking.'); await ui.submit();
  assert.deepEqual(ui.requests.at(-1),{url:'/nodes/thinkcentre-llm/execute',data:{capability:'Inference',task:{type:'generate_text',prompt:'Explain networking.'}}});
  assert.match(ui.el('output').textContent,/Model answer/);
  assert.equal(ui.el('result-status').textContent,'OUTPUT RECEIVED');
  assert.equal(ui.el('ledger-count').textContent,'01 / COMPLETE');
});

test('task controls use selected node capabilities and encoded URL paths',async t => {
  const ui = fixture(t); await tick(); ui.select('field/node 01');
  assert.equal(ui.q('[data-action="generate_text"]').disabled,true);
  ui.el('test').click(); await tick();
  assert.equal(ui.requests.at(-1).data.task.type,'get_system_info');
  assert.equal(ui.requests.at(-1).data.capability,'Tools');
  ui.action('echo'); ui.input('Hello network.'); await ui.submit();
  assert.equal(ui.requests.at(-1).url,'/nodes/field%2Fnode%2001/execute');
  assert.equal(ui.el('output').textContent,'Hello network.');
  ui.action('store_observation'); ui.input('Fresh deer tracks.'); await ui.submit();
  assert.equal(ui.requests.at(-1).data.capability,'Storage');
  assert.equal(ui.requests.at(-1).data.task.text,'Fresh deer tracks.');
  ui.action('list_observations'); await ui.submit();
  assert.equal(ui.el('input').hidden,true);
  assert.equal(ui.d.querySelectorAll('.hp-observation').length,2);
  assert.equal(ui.d.querySelectorAll('.hp-observation time').length,1);
  assert.match(ui.el('output').textContent,/<script>alert/);
  assert.equal(ui.el('output').querySelectorAll('script').length,0);
});

test('offline and lost controller states hide stale telemetry and recover',async t => {
  const ui = fixture(t); await tick(); ui.select(ui.state.nodes[2].info.id);
  assert.equal(ui.el('run').disabled,true); assert.equal(ui.el('test').disabled,true);
  assert.match(ui.el('cpu-reading').textContent,/—/);
  assert.equal(ui.el('cpu-meter').hasAttribute('aria-valuenow'),false);
  assert.equal(ui.el('offline-warning').hidden,false);
  ui.select('thinkcentre-llm'); ui.input('Ready.'); ui.state.down = true; await ui.refresh();
  assert.equal(ui.el('connection-notice').hidden,false);
  assert.equal(ui.el('run').disabled,true);
  assert.match(ui.el('cpu-reading').textContent,/—/);
  assert.equal(ui.el('controller-mode').textContent,'LINK LOST');
  ui.state.down = false; await ui.refresh();
  assert.equal(ui.el('connection-notice').hidden,true);
  assert.equal(ui.el('run').disabled,false);
  assert.match(ui.el('cpu-reading').textContent,/28.4/);
});

test('failed upstream, mismatched replies and timeouts never count as completion',async t => {
  const ui = fixture(t); await tick(); ui.input('Test.');
  ui.state.postStatus = 502; await ui.submit();
  assert.match(ui.el('output').textContent,/HTTP 502/);
  assert.equal(ui.el('result-status').textContent,'REQUEST FAILED');
  ui.state.postStatus = 200; ui.state.wrongAgent = true; await ui.submit();
  assert.match(ui.el('output').textContent,/does not match/);
  assert.equal(ui.el('ledger-count').textContent,'00 / COMPLETE');
  const timed = fixture(t,{timeout:true}); await tick(); timed.input('Test timeout.'); await timed.submit();
  assert.match(timed.el('output').textContent,/timed out after 200 seconds/);
  assert.equal(timed.el('run').disabled,false);
});

test('busy guard prevents duplicate requests, node changes and history replay',async t => {
  const ui = fixture(t); await tick(); ui.input('First request.'); await ui.submit();
  let release; ui.state.held = new Promise(resolve => {release = resolve;});
  ui.input('Second request.'); await ui.submit();
  const count = ui.requests.length;
  assert.equal(ui.el('run').disabled,true);
  ui.select('field/node 01'); await ui.submit();
  ui.el('events').querySelector('button')?.click();
  assert.equal(ui.requests.length,count);
  assert.equal(ui.el('selected-name').textContent,'thinkcentre-llm');
  assert.equal(ui.el('result-status').textContent,'TASK ROUTING');
  release(); ui.state.held = null; await tick();
  assert.equal(ui.el('run').disabled,false);
  assert.match(ui.el('result-meta').textContent,/thinkcentre-llm/);
});

test('drafts, session history, replay and changed capabilities remain consistent',async t => {
  const saved = {nodeId:'old-node',action:'echo',output:'Stored response',ok:true,elapsed:15,timestamp:Date.now()};
  const ui = fixture(t,{history:[saved,{action:'__proto__',nodeId:'bad',output:'bad'}]}); await tick();
  assert.equal(ui.el('output').textContent,'Stored response');
  ui.input('Keep this prompt.'); ui.action('echo'); ui.input('Keep this echo.'); ui.action('generate_text');
  assert.equal(ui.el('input').value,'Keep this prompt.');
  await ui.submit();
  ui.el('events').querySelectorAll('button')[1].click();
  assert.equal(ui.el('output').textContent,'Stored response');
  ui.state.nodes[0].info.capabilities = ['Tools']; await ui.refresh();
  assert.equal(ui.q('[data-action="generate_text"]').disabled,true);
  assert.equal(ui.q('[data-action="get_system_info"]').getAttribute('aria-pressed'),'true');
  assert.equal(ui.el('input').hidden,true);
});

test('invalid fleet payloads disable tasks; empty fleet has no fake nodes',async t => {
  const ui = fixture(t); await tick(); ui.state.malformed = true; await ui.refresh();
  assert.equal(ui.el('test').disabled,true);
  assert.equal(ui.el('controller-mode').textContent,'LINK LOST');
  ui.state.malformed = false; ui.state.nodes = []; await ui.refresh();
  assert.equal(ui.d.querySelectorAll('[data-node-id]').length,0);
  assert.match(ui.el('nodeList').textContent,/NO AGENTS REGISTERED/);
  assert.equal(ui.el('online-count').textContent,'00');
});

test('fonts and icons are bundled with no external resource dependencies',t => {
  const ui = fixture(t);
  assert.equal(ui.d.querySelectorAll('script[src],link[href],img[src]').length,0);
  assert.match(html,/data:font\/woff2;base64,/);
  assert.match(html,/font-family:"Hive95"/);
  assert.match(html,/font-family:"HiveDot"/);
  const ids = Array.from(ui.d.querySelectorAll('[id]')).map(element => element.id);
  assert.equal(new Set(ids).size,ids.length);
  ui.d.querySelectorAll('use').forEach(use => assert.ok(ui.d.getElementById(use.getAttribute('href').slice(1))));
});
