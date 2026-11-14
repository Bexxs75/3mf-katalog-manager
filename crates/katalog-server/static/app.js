'use strict';
let csrfToken = null;
let mustChange = false;
const element = id => document.getElementById(id);
const error = message => { element('error').textContent = message || ''; };
function view(name) {
  for (const id of ['login', 'change', 'catalog']) element(`${id}-view`).hidden = id !== name;
  element('notice').textContent = '';
  element('cancel-change').hidden = mustChange;
}
async function request(path, body) {
  const headers = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (csrfToken) headers['X-CSRF-Token'] = csrfToken;
  const response = await fetch(path, { method: body === undefined ? 'GET' : 'POST', headers, credentials: 'same-origin', body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.json();
  if (!response.ok) {
    if (response.status === 401 && path !== '/api/login') { csrfToken = null; view('login'); }
    if (data.code === 'password_change_required') { mustChange = true; view('change'); }
    throw new Error(data.message || `Anfrage fehlgeschlagen (${response.status})`);
  }
  return data;
}
async function catalog() {
  view('catalog');
  element('models').replaceChildren();
  const models = await request('/api/invoke/list_file_summaries', {});
  for (const model of models) {
    const row = document.createElement('tr');
    for (const value of [model.name, model.fileType, model.fileSizeBytes, model.printStatus]) {
      const cell = document.createElement('td');
      cell.textContent = value == null ? '' : String(value);
      row.append(cell);
    }
    element('models').append(row);
  }
  element('empty').hidden = models.length !== 0;
}
async function authenticated(data) {
  csrfToken = data.csrfToken;
  mustChange = data.mustChangePassword;
  if (mustChange) view('change'); else await catalog();
}
function submit(id, action) {
  element(id).addEventListener('submit', async event => {
    event.preventDefault(); error('');
    const button = event.currentTarget.querySelector('button'); button.disabled = true;
    try { await action(); } catch (e) { error(e.message); } finally { button.disabled = false; }
  });
}
submit('login', async () => {
  const password = element('password').value;
  element('password').value = '';
  await authenticated(await request('/api/login', { password }));
});
submit('change', async () => {
  if (element('new').value !== element('repeat').value) throw new Error('Die neuen Passwörter stimmen nicht überein.');
  const data = await request('/api/password', { current: element('current').value, new: element('new').value });
  element('change').reset();
  await authenticated(data);
  element('notice').textContent = 'Passwort geändert. Andere Sitzungen wurden beendet.';
});
element('open-change').addEventListener('click', () => { error(''); view('change'); });
element('cancel-change').addEventListener('click', () => { error(''); view('catalog'); });
element('logout').addEventListener('click', async () => {
  error('');
  try { await request('/api/logout', {}); csrfToken = null; mustChange = false; element('models').replaceChildren(); view('login'); }
  catch (e) { error(e.message); }
});
(async () => {
  try { const data = await request('/api/session'); if (data.authenticated) await authenticated(data); else view('login'); }
  catch (e) { view('login'); error(e.message); }
})();
