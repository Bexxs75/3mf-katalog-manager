import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { SpoolImage } from './SpoolImage';
import { invalidateSpoolImage } from '../lib/spoolImages';
import { login, authenticationFailed } from '../lib/webSession';
import type { FilamentSpool } from '../types';
const observers: {cb: (entries: {isIntersecting:boolean}[])=>void; disconnected:boolean}[]=[];
let calls: string[]=[];
let active=0, peak=0;
let release: (()=>void)[]=[];
let hold=false;
function picture(id:string) { return <SpoolImage key={id} spool={{id,hasImage:true,imagePng:null} as FilamentSpool} className="image" fallback={<span>{id}</span>} />; }
beforeEach(async()=>{
  vi.stubEnv('VITE_TARGET','web'); invalidateSpoolImage(); observers.length=0; calls=[]; release=[]; hold=false; active=peak=0;
  vi.stubGlobal('IntersectionObserver', class {
    entry: typeof observers[number];
    constructor(cb: typeof observers[number]['cb']) { this.entry={cb,disconnected:false}; observers.push(this.entry); }
    observe() {} disconnect() {this.entry.disconnected=true;}
  });
  vi.stubGlobal('fetch',vi.fn(async(url:string,opts?:RequestInit)=>{
    if (!url.includes('/invoke/')) return Response.json({authenticated:true,writable:false,csrfToken:'t'});
    const {id}=JSON.parse(String(opts?.body)); calls.push(id); active++; peak=Math.max(peak,active);
    if(hold) await new Promise<void>(resolve=>release.push(resolve));
    active--; return Response.json({imagePng:'aW1hZ2U='});
  }));
  await login('pw');
});
afterEach(()=>{cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs();});
it('loads only visible images, caches across remounts and invalidates after edits',async()=>{
  const view=render(picture('one')); expect(calls).toEqual([]);
  await act(async()=>{observers[0].cb([{isIntersecting:true}]);});
  await waitFor(()=>expect(document.querySelector('img')).toHaveAttribute('src','data:image/png;base64,aW1hZ2U='));
  expect(calls).toEqual(['one']); view.unmount();
  render(picture('one')); await act(async()=>{observers[1].cb([{isIntersecting:true}]);});
  expect(calls).toEqual(['one']);
  await act(async()=>{invalidateSpoolImage('one');});
  await act(async()=>{observers[observers.length-1].cb([{isIntersecting:true}]);});
  expect(calls).toEqual(['one','one']);
});
it('limits ten visible images to four requests and drops queued unmounted images',async()=>{
  hold=true;
  const view=render(<>{Array.from({length:10},(_,i)=>picture(String(i)))}</>);
  act(()=>{observers.forEach(o=>o.cb([{isIntersecting:true}]));});
  await waitFor(()=>expect(calls).toHaveLength(4)); expect(peak).toBe(4);
  view.unmount(); await act(async()=>{release.splice(0).forEach(fn=>fn());});
  expect(calls).toHaveLength(4);
});
it('does not miss an image becoming visible again while its queued request is cancelled',async()=>{
  hold=true;
  render(<>{Array.from({length:5},(_,i)=>picture(String(i)))}</>);
  act(()=>{observers.forEach(o=>o.cb([{isIntersecting:true}]));});
  await waitFor(()=>expect(calls).toHaveLength(4));
  act(()=>{observers[4].cb([{isIntersecting:false}]);});
  // Resolve one slot and show it again before queued work completes.
  await act(async()=>{release.shift()!();});
  act(()=>{observers[4].cb([{isIntersecting:true}]);});
  await waitFor(()=>expect(calls).toContain('4'));
  await act(async()=>{release.splice(0).forEach(fn=>fn());});
  expect(screen.queryByText('4')).toBeNull();
});
it('shows a visible error after a failed image read',async()=>{
  const {getMetadataError,clearMetadataError}=await import('../lib/metadataErrors'); clearMetadataError();
  vi.stubGlobal('fetch',vi.fn(async()=>Response.json({message:'image failure',expected:true},{status:404})));
  render(picture('bad'));
  await act(async()=>{observers[0].cb([{isIntersecting:true}]);});
  expect(getMetadataError()).toBe('image failure');
});
it('releases image elements outside the viewport and reuses the cache on return',async()=>{
  render(picture('one'));
  await act(async()=>{observers[0].cb([{isIntersecting:true}]);});
  expect(document.querySelector('img')).not.toBeNull();
  act(()=>{observers[0].cb([{isIntersecting:false}]);});
  expect(document.querySelector('img')).toBeNull();
  await act(async()=>{observers[0].cb([{isIntersecting:true}]);});
  expect(document.querySelector('img')).not.toBeNull(); expect(calls).toEqual(['one']);
});
