import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, canManageFiles, getSession, getSnapshot, login } from './webSession';
import { invoke } from './transport';
import { emptyWebTrash, fileOp, fileOpsBatch, subscribeFileOpsRefresh } from './fileOps';
let calls: {command:string; args:Record<string, unknown>}[];
let enabled = true;
let respond: (command:string) => Response;
beforeEach(async () => {
  calls=[]; enabled=true; respond=() => Response.json(null);
  vi.stubGlobal('fetch', vi.fn(async (url:string, options?:RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({authenticated:true,writable:true,fileOps:enabled,csrfToken:'token'});
    const command=url.split('/').pop()!; calls.push({command,args:JSON.parse(String(options?.body))}); return respond(command);
  }));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.useRealTimers(); vi.unstubAllGlobals(); });
it('requires fileOps separately and revokes immediately on restart_required', async () => {
  expect(canManageFiles()).toBe(true); enabled=false; await getSession();
  await expect(invoke('rename_file',{fileId:'1',name:'new'})).rejects.toMatchObject({expected:true}); expect(calls).toHaveLength(0);
  enabled=true; await getSession(); respond=()=>Response.json({kind:'restart_required',opIds:['abc123']},{status:503});
  await expect(fileOp('rename_file',{fileId:'1',name:'new'})).rejects.toMatchObject({message:expect.stringContaining('abc123')});
  expect(canManageFiles()).toBe(false); expect(getSnapshot().fileOps).toBe(false);
});
it('refreshes ambiguous failures without repeating writes', async () => {
  const refresh=vi.fn(); const off=subscribeFileOpsRefresh(refresh);
  respond=()=>{throw new TypeError('network');};
  await expect(fileOp('rename_file',{fileId:'1',name:'new'})).rejects.toBeDefined();
  expect(calls).toHaveLength(1); expect(refresh).toHaveBeenCalledOnce(); off();
});
it('chunks batches at 200, shows each failed ID and stops unstarted work', async () => {
  respond=()=>Response.json([{id:'1',ok:true},{id:'2',ok:false,kind:'cross_device'}]);
  await expect(fileOpsBatch('delete_files',Array.from({length:201},(_,i)=>String(i+1)))).rejects.toMatchObject({message:expect.stringMatching(/2:[\s\S]*201:/)});
  expect((calls[0].args.fileIds as string[])).toHaveLength(200); expect(calls).toHaveLength(1);
});
it('empties only the frozen token snapshot and reports every block', async () => {
  vi.useFakeTimers();
  respond=command=>Response.json(command==='empty_trash_start'?{token:'snapshot',remaining:201}:calls.filter(c=>c.command==='empty_trash_block').length===1?{results:[{id:'1',ok:true}],remaining:1}:{results:[{id:'2',ok:true}],remaining:0});
  const progress=vi.fn(); const pending=emptyWebTrash({count:201,bytes:4,maxDeletedAt:'date'},'secret',progress);
  await vi.runAllTimersAsync(); await pending;
  expect(calls.map(c=>c.command)).toEqual(['empty_trash_start','empty_trash_block','empty_trash_block']);
  expect(calls[0].args).toEqual({expectedCount:201,maxDeletedAt:'date',password:'secret'});
  expect(calls[1].args).toEqual({token:'snapshot'}); expect(progress).toHaveBeenLastCalledWith(201,201);
  expect(JSON.stringify(getSnapshot())).not.toContain('secret');
});
it('retries an explicit pre-start rate limit once and uses a fresh capability check', async () => {
  vi.useFakeTimers(); respond=()=>calls.length===1?Response.json({kind:'rate_limited'},{status:429,headers:{'Retry-After':'1'}}):Response.json(null);
  const pending=fileOp('rename_file',{fileId:'1',name:'new'}); await vi.runAllTimersAsync(); await pending; expect(calls).toHaveLength(2);
  calls=[];
  const revoked=fileOp('rename_file',{fileId:'1',name:'new'}); const rejection=expect(revoked).rejects.toBeDefined();
  await vi.advanceTimersByTimeAsync(0); authenticationFailed(401); await vi.runAllTimersAsync(); await rejection; expect(calls).toHaveLength(1);
});
it('never retries backup or restart failures even with an erroneous Retry-After', async () => {
  respond=()=>Response.json({kind:'backup_failed'},{status:503,headers:{'Retry-After':'0'}});
  await expect(fileOp('delete_files',{fileIds:['1']})).rejects.toBeDefined(); expect(calls).toHaveLength(1);
});
it('does not continue the snapshot on not_started or an uncertain block response', async () => {
  respond=command=>command==='empty_trash_start'?Response.json({token:'snapshot',remaining:2}):Response.json({results:[{id:'1',ok:false,kind:'not_started'}],remaining:1});
  const progress=vi.fn(); await expect(emptyWebTrash({count:2,bytes:0,maxDeletedAt:null},'secret',progress)).rejects.toBeDefined();
  expect(calls).toHaveLength(2); expect(progress).toHaveBeenLastCalledWith(1,2);
});
it('revokes fileOps for restart_required inside an HTTP 200 batch, even after another failure', async () => {
  respond=()=>Response.json([{id:'1',ok:false,kind:'conflict'},{id:'2',ok:false,kind:'restart_required'}]);
  await expect(fileOp('delete_files',{fileIds:['1','2']})).rejects.toBeDefined(); expect(canManageFiles()).toBe(false);
});
it('does not replay a pending write after logout and a new login', async () => {
  vi.useFakeTimers(); respond=()=>Response.json({kind:'rate_limited'},{status:429,headers:{'Retry-After':'1'}});
  const pending=fileOp('rename_file',{fileId:'1',name:'new'}); const rejected=expect(pending).rejects.toBeDefined();
  await vi.advanceTimersByTimeAsync(0); authenticationFailed(401);
  vi.mocked(fetch).mockResolvedValueOnce(Response.json({authenticated:true,writable:true,fileOps:true,csrfToken:'new-token'}));
  await login('password'); expect(canManageFiles()).toBe(true);
  await vi.runAllTimersAsync(); await rejected; expect(calls).toHaveLength(1);
});
it('resets file management on password change requirement and logout', async () => {
  authenticationFailed(403,'password_change_required'); expect(canManageFiles()).toBe(false); expect(getSnapshot().fileOps).toBe(false);
  await login('password'); expect(canManageFiles()).toBe(true); authenticationFailed(401); expect(canManageFiles()).toBe(false);
});
