// Component-state regression, without a browser dependency. Run: node scripts/test-objects3d-ui.cjs
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
class Element {
  constructor(tag='div') { this.tag=tag; this.children=[]; this.value=''; this.textContent=''; this.hidden=false; this.open=false; this.events={}; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children=nodes; }
  setAttribute(key,value) { this[key]=value; }
  addEventListener(type,fn) { this.events[type]=fn; }
  showModal() { this.open=true; }
  close() { this.open=false; this.events.close?.(); }
  focus() {}
}
const html=fs.readFileSync('src/dashboard.html','utf8');
const elements=new Map([...html.matchAll(/id="([^"]+)"/g)].map(m=>[m[1],new Element()]));
const el=id=>{assert.ok(elements.has(id),`missing element ${id}`);return elements.get(id);};
let data=Array.from({length:22},(_,i)=>({dataset:`dataset-${i}`,revision:String(i).padStart(64,'a'),objects:4,bytes:1234,hits:8}));
let failure=false,unauthorized=false,deleted=0;
const context={console,document:{createElement:tag=>new Element(tag)},el,authenticated:false,size:n=>`${n} B`,requireCatalogLogin:()=>context.authenticated,
  request:async(url,options={})=>{
    if(failure)throw Error('offline');
    if(unauthorized)return {status:401,ok:false};
    if(options.method==='DELETE') { deleted++;const item=data.find(r=>url.endsWith('/'+r.dataset+'/'+r.revision));assert.ok(item);data=data.filter(r=>r!==item);return {status:200,ok:true,json:async()=>({deleted_objects:item.objects,deleted_bytes:item.bytes})}; }
    return {status:200,ok:true,json:async()=>({datasets:data})};
  },setAuth:value=>{context.authenticated=value;return context.refreshObjects3d();}};
context.window=context;vm.createContext(context);vm.runInContext(fs.readFileSync('src/objects3d.js','utf8'),context);
(async()=>{
  await context.refreshObjects3d();assert.match(el('objectsRows').children[0].children[0].textContent,/登录/);
  context.authenticated=true;await context.refreshObjects3d();assert.equal(el('objectsRows').children.length,20);
  el('objectsNext').onclick();assert.equal(el('objectsRows').children.length,2);
  el('objectsSearch').value='dataset-0';el('objectsSearch').oninput();assert.equal(el('objectsRows').children.length,1);
  const row=el('objectsRows').children[0];assert.equal(row.children[4].textContent,'8');
  row.children[5].children[0].onclick();assert.equal(el('objectsConfirm').open,true);assert.equal(el('objectsTargetDataset').textContent,'dataset-0');
  el('objectsCancel').onclick();assert.equal(deleted,0);assert.equal(el('objectsConfirm').open,false);
  row.children[5].children[0].onclick();await el('objectsDelete').onclick();assert.equal(deleted,1);assert.equal(data.length,21);assert.match(el('objectsStatus').textContent,/已清理/);
  el('objectsSearch').value='';el('objectsSearch').oninput();failure=true;await context.refreshObjects3d();assert.match(el('objectsStatus').textContent,/读取失败/);
  failure=false;await context.refreshObjects3d();assert.equal(el('objectsRows').children.length,20);
  unauthorized=true;await context.refreshObjects3d();assert.equal(context.authenticated,false);assert.match(el('objectsRows').children[0].children[0].textContent,/登录/);
  // Late responses must not repopulate a signed-out account's view.
  unauthorized=false;context.authenticated=true;let resolve;
  context.request=()=>new Promise(r=>resolve=r);const pending=context.refreshObjects3d();await context.setAuth(false);
  resolve({ok:true,status:200,json:async()=>({datasets:data})});await pending;
  assert.match(el('objectsRows').children[0].children[0].textContent,/登录/);
  assert.ok(html.includes('src="assets/objects3d.js"'));
  console.log('PASS: login, pagination, search, cancel, exact-version cleanup, refresh, errors, expiry, stale-response isolation');
})().catch(e=>{console.error(e);process.exitCode=1;});
