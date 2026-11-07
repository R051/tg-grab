const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const ts = require('typescript');
const vue = require('vue');
const pinia = require('pinia');
const root = path.resolve(__dirname, '..');
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise, resolve, reject}; }
function load(file, stubs = {}, names = [], globals = {}) {
  let source = fs.readFileSync(path.join(root, file), 'utf8');
  if (file.endsWith('.vue')) source = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
  if (names.length) source += '\nexport {' + names.join(',') + '};';
  const code = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const mod = {exports:{}};
  const context = {module:mod,exports:mod.exports,require:name => name in stubs ? stubs[name] : name.endsWith('.vue') ? {} : require(name),console,Date,setTimeout,clearTimeout,window:{confirm:()=>true},...globals};
  vm.runInNewContext(code,context,{filename:file});
  return mod.exports;
}
function newStore(invoke, globals = {}) {
  pinia.setActivePinia(pinia.createPinia());
  const handlers = {}, connections = [];
  let reconnect;
  const backend = {invoke,on:async (event,handler)=>{handlers[event]=handler;return ()=>delete handlers[event];},onReconnect:handler=>{reconnect=handler;return ()=>{};},onConnection:handler=>{connections.push(handler);return ()=>{};}};
  const store = load('src/stores/app.ts',{'../lib/backend':backend,'../i18n':{setLocale:()=>{}}},[],globals).useAppStore();
  return {store,handlers,reconnect:()=>reconnect?.()};
}
const empty = () => ({pending:[],active:null,activeTasks:[],results:[]});
const reply = command => command==='get_queue' ? empty() : command==='get_status' ? {configured:true,authorized:true} : command==='get_settings' ? {lang:'zh-CN'} : [];
function view(file, invoke, store, names) {
  const messages = [];
  const exports = load(file,{'vue':{...vue,onUnmounted:()=>{}},'../lib/backend':{invoke},'../lib/errors':load('src/lib/errors.ts'),'../stores/app':{useAppStore:()=>store},'../i18n':{setLocale:()=>{}},'vue-i18n':{useI18n:()=>({t:(key,args)=>args?key+JSON.stringify(args):key,locale:vue.ref('zh-CN')})},'naive-ui':{useMessage:()=>Object.fromEntries(['success','error','info','warning'].map(key=>[key,(...args)=>messages.push([key,...args])]))}},names);
  return {...exports,messages};
}
const page = (name, videos=[], extra={}) => ({chat:{rawId:name==='A'?1:2,title:name},videos,nextOffset:0,nextDate:null,exhausted:true,...extra});
const video = (id, name='A') => ({msgId:id,url:`https://t.me/${name}/${id}`,name:`${name}-${id}`,hasThumb:false,downloaded:false,date:'2026-11-07T12:00:00+08:00'});
const historyStore = () => vue.reactive({pending:[],activeTasks:[],results:[],files:[],refreshQueue:async()=>{}});

test('failed initialization retries without duplicating event subscriptions', async()=>{
  let fail=true,subscriptions=0;
  const {store,handlers}=newStore(async command=>{if(fail)throw Error('offline');return reply(command);});
  await assert.rejects(store.init());
  subscriptions=Object.keys(handlers).length;
  fail=false;await store.init();await store.init();
  assert.equal(store.initState,'ready');assert.equal(Object.keys(handlers).length,subscriptions);
  store.dispose();assert.equal(Object.keys(handlers).length,0);
});

test('queue requests coalesce and stale snapshots cannot erase a completion event',async()=>{
  let slow=false,calls=0;const first=deferred();let done=false;
  const {store,handlers}=newStore(async command=>{
    if(command!=='get_queue')return reply(command);
    if(!slow)return empty();
    calls++;if(calls===1)return first.promise;
    return {...empty(),results:done?[{id:1,url:'A',status:'success'}]:[]};
  });
  await store.init();slow=true;
  const a=store.refreshQueue(),b=store.refreshQueue();assert.equal(calls,1);
  done=true;handlers['queue:result']({id:1,url:'A',status:'success'});
  first.resolve(empty());await Promise.all([a,b]);
  assert.equal(store.results.length,1);assert.equal(store.results[0].id,1);assert.equal(calls,2);
  handlers['queue:result']({id:1,url:'A',status:'success'});assert.equal(store.results.length,1);
});

test('parallel tasks keep independent speed samples and ignore terminal progress',async()=>{
  let now=0;class TestDate extends Date {static now(){return now;}}
  const {store,handlers}=newStore(async command=>reply(command),{Date:TestDate});await store.init();
  const progress=(id,current)=>({id,url:String(id),current,total:10000,percent:0,name:String(id)});
  store.progress(progress(1,100));store.progress(progress(2,500));now=1000;
  store.progress(progress(1,1100));store.progress(progress(2,700));
  assert.equal(store.speeds[1],1000);assert.equal(store.speeds[2],200);assert.equal(store.speed,1200);
  handlers['queue:result']({id:1,url:'1',status:'success'});store.progress(progress(1,1200));
  assert.equal(store.activeTasks.length,1);assert.equal(store.speed,200);
});

test('settings hydrate asynchronously while preserving only edited fields',async()=>{
  const store=vue.reactive({settings:null,status:{authorized:false}});
  const form=view('src/views/SettingsView.vue',async()=>null,store,['apiId','apiHash','concurrency','lang','markTouched']);
  form.apiId.value='edited';form.markTouched('apiId');
  store.settings={api_id:123,api_hash:'saved',proxy_url:'proxy',concurrency:3,lang:'en'};await vue.nextTick();
  assert.equal(form.apiId.value,'edited');assert.equal(form.apiHash.value,'saved');assert.equal(form.concurrency.value,3);assert.equal(form.lang.value,'en');
});

test('login recognizes the wire status and preserves retry UI after an invalid code',async()=>{
  let invalid=true,refreshes=0;
  const form=view('src/views/SettingsView.vue',async()=>{if(invalid)throw Error('SIGN_IN: invalid code');return {status:'ok',user:'demo'};},{status:{authorized:false},refreshStatus:async()=>refreshes++},['code','step','verifyCode']);
  form.code.value='12345';form.step.value='code';await form.verifyCode();assert.equal(form.step.value,'code');
  invalid=false;await form.verifyCode();assert.equal(form.step.value,'phone');assert.equal(refreshes,1);
});

test('cleared thumbnail bytes use the shared object contract',async()=>{
  const form=view('src/views/SettingsView.vue',async()=>({freed:1048576}),{status:{}},['clearThumbs']);
  await form.clearThumbs();assert.match(form.messages[0][1],/"mb":"1.0"/);
});

test('a slower previous channel cannot overwrite the current history',async()=>{
  const a=deferred(),b=deferred();
  const history=view('src/views/HistoryView.vue',(command,args)=>command==='history_page'?(args.link==='A'?a.promise:b.promise):Promise.resolve([]),historyStore(),['link','browse','chatTitle']);
  history.link.value='A';const pa=history.browse();history.link.value='B';const pb=history.browse();
  b.resolve(page('B'));await pb;a.resolve(page('A'));await pa;assert.equal(history.chatTitle.value,'B');
});

test('channel changes start fresh thumbnail requests and discard old images',async()=>{
  const a=deferred(),b=deferred(),calls=[];
  const history=view('src/views/HistoryView.vue',(command,args)=>{
    if(command==='history_page')return Promise.resolve(page(args.link,[{...video(1,args.link),hasThumb:true}]));
    if(command==='history_thumbs'){calls.push(args.link);return args.link==='A'?a.promise:b.promise;}
    return Promise.resolve([]);
  },historyStore(),['link','browse','thumbs']);
  history.link.value='A';await history.browse();history.link.value='B';await history.browse();
  b.resolve([{msgId:1,data:'B'}]);await tick();a.resolve([{msgId:1,data:'A'}]);await tick();
  assert.deepEqual(calls,['A','B']);assert.equal(history.thumbs.value['2_1'],'data:image/jpeg;base64,B');assert.equal(history.thumbs.value['1_1'],undefined);
});

test('same-day pagination follows the cursor even if the input changes',async()=>{
  const calls=[];
  const history=view('src/views/HistoryView.vue',(command,args)=>{
    if(command!=='history_page')return Promise.resolve([]);
    calls.push(args);
    return Promise.resolve(calls.length===1?page('A',Array.from({length:120},(_,i)=>video(1000-i)),{nextOffset:881,nextDate:'2026-11-07',exhausted:false}):page('A',[video(880)]));
  },historyStore(),['link','dateFilter','browse','loadPrevDay','videos','exhausted']);
  history.link.value='A';history.dateFilter.value=new Date(2026,10,7).getTime();await history.browse();
  history.link.value='B';await history.loadPrevDay();
  assert.equal(calls[1].link,'A');assert.equal(calls[1].offsetId,881);assert.equal(calls[1].date,'2026-11-07');assert.equal(history.videos.value.length,121);assert.equal(history.exhausted.value,true);
});

test('empty history pages remain pageable without a fixed empty-day cutoff',async()=>{
  let calls=0;
  const history=view('src/views/HistoryView.vue',(command,args)=>{
    if(command!=='history_page')return Promise.resolve([]);
    calls++;return Promise.resolve(calls===1?page('A',[],{nextOffset:700,exhausted:false}):page('A',[video(699)]));
  },historyStore(),['link','browse','loadPrevDay','videos','exhausted']);
  history.link.value='A';await history.browse();assert.equal(history.exhausted.value,false);await history.loadPrevDay();assert.equal(history.videos.value.length,1);
});

test('pending and active snapshots produce one task row per id',()=>{
  const store=vue.reactive({pending:[{id:1,url:'A'}],activeTasks:[{id:1,url:'A'}],results:[],files:[],speeds:{}});
  const downloads=view('src/views/DownloadView.vue',async()=>null,store,['taskRowsRaw']);
  assert.equal(downloads.taskRowsRaw.value.length,1);assert.equal(downloads.taskRowsRaw.value[0].kind,'active');
});

test('HTTPS uses WSS and resynchronization waits until the new socket opens',async()=>{
  const sockets=[],timers=new Map();let timerId=0,reconnects=0;
  class Socket {constructor(url){this.url=url;sockets.push(this);}close(){this.onclose?.();}}
  const backend=load('src/lib/backend.ts',{},[],{WebSocket:Socket,location:{protocol:'https:',host:'example.test'},localStorage:{getItem:()=>null},window:{setTimeout:fn=>{timers.set(++timerId,fn);return timerId;},clearTimeout:id=>timers.delete(id)}});
  backend.onReconnect(()=>reconnects++);const off=await backend.on('queue:changed',()=>{});
  assert.match(sockets[0].url,/^wss:/);sockets[0].onopen();assert.equal(reconnects,1);
  sockets[0].onclose();const pending=[...timers.values()][0];timers.clear();pending();assert.equal(reconnects,1);
  sockets[1].onopen();assert.equal(reconnects,2);
  assert.match(backend.fileUrl('channel\\day\\a b.mp4'),/^\/files\/channel\/day\/a%20b.mp4/);off();assert.equal(timers.size,0);
});
