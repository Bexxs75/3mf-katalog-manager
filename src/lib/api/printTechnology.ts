import { invoke } from '../transport';
import { parsePrintTechnologyState, type PrintTechnology } from '../printTechnology';

export async function getPrintTechnology() {
  return parsePrintTechnologyState(await invoke<unknown>('get_print_technology'));
}
/** Stores the choice; also marks the question as answered. */
export async function setPrintTechnology(value: PrintTechnology) {
  return parsePrintTechnologyState(await invoke<unknown>('set_print_technology', { value }));
}
