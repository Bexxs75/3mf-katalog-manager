import { invoke } from '@tauri-apps/api/core';

/** Tags the app would assign to this model automatically today (names as stored). */
export function listSuggestedAutoTags(fileId: string) {
  return invoke<string[]>('list_suggested_auto_tags', { fileId });
}
/** Stops the automatic assignment of a tag; tags added by hand are not affected. */
export function blockAutoTag(tag: string) {
  return invoke<void>('block_auto_tag', { tag });
}
export function unblockAutoTag(tag: string) {
  return invoke<void>('unblock_auto_tag', { tag });
}
export function listBlockedAutoTags() {
  return invoke<string[]>('list_blocked_auto_tags');
}
/** Whether the app assigns tags automatically at all (on unless switched off). */
export function getAutoTagging() {
  return invoke<boolean>('get_auto_tagging');
}
export function setAutoTagging(enabled: boolean) {
  return invoke<void>('set_auto_tagging', { enabled });
}
