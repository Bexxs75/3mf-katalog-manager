import { invoke } from '../transport';
import type { ThingDetail, ThingiverseSearch, ThingiverseSettings } from '../../types';

export function getThingiverseSettings() {
  return invoke<ThingiverseSettings>('get_thingiverse_settings');
}
export function setThingiverseEnabled(enabled: boolean) {
  return invoke<void>('set_thingiverse_enabled', { enabled });
}
/** With a key: checks it and, if it works, stores it. Without: checks the stored one. Resolves with the user name. */
export function testThingiverseToken(token?: string) {
  return invoke<{ ok: boolean; userName: string | null }>('test_thingiverse_token', { token: token ?? null });
}
export function removeThingiverseToken() {
  return invoke<void>('remove_thingiverse_token');
}
export function thingiverseSearch(query: string, page: number) {
  return invoke<ThingiverseSearch>('thingiverse_search', { query, page });
}
export function thingiverseThing(thingId: number) {
  return invoke<ThingDetail>('thingiverse_thing', { thingId });
}
/** A `data:` address, or null when the thing has no image. The backend takes the address from its own memory. */
export function thingiverseImage(thingId: number, size: 'thumb' | 'large') {
  return invoke<string | null>('thingiverse_image', { thingId, size });
}
export function openThingiverseThing(thingId: number) {
  return invoke<void>('open_thingiverse_thing', { thingId });
}
