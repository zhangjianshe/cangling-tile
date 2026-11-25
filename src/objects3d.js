/* Administrator-only 3D cache management; all labels from the server use textContent. */
(() => {
  let rows = [], page = 1, sequence = 0, target = null, deleting = false;
  const limit = 20, dialog = el('objectsConfirm');
  function render() {
    const query = el('objectsSearch').value.trim().toLowerCase();
    const filtered = rows.filter(r => `${r.dataset} ${r.revision}`.toLowerCase().includes(query));
    const pages = Math.max(1, Math.ceil(filtered.length / limit));
    page = Math.min(page, pages);
    el('objectsRows').replaceChildren();
    for (const item of filtered.slice((page - 1) * limit, page * limit)) {
      const tr = document.createElement('tr');
      const cell = (value, cls = '') => {
        const td = document.createElement('td'); td.textContent = value; td.className = cls; tr.append(td); return td;
      };
      cell(item.dataset);
      const revision = document.createElement('code');
      revision.textContent = item.revision; revision.title = item.revision;
      cell('').append(revision);
      cell(item.objects.toLocaleString(), 'number');
      cell(size(item.bytes), 'number').title = `${item.bytes.toLocaleString()} 字节`;
      cell(item.hits.toLocaleString(), 'number');
      const button = document.createElement('button');
      button.type = 'button'; button.className = 'button secondary'; button.textContent = '清理';
      button.setAttribute('aria-label', `清理 ${item.dataset} 版本 ${item.revision}`);
      button.onclick = () => {
        if (!requireCatalogLogin('请先登录后再清理三维缓存')) return;
        target = item;
        el('objectsTargetDataset').textContent = item.dataset;
        el('objectsTargetRevision').textContent = item.revision;
        el('objectsTargetSize').textContent = `${item.objects.toLocaleString()} 个文件 · ${size(item.bytes)}`;
        el('objectsConfirmStatus').textContent = '';
        dialog.showModal(); el('objectsCancel').focus();
      };
      cell('').append(button); el('objectsRows').append(tr);
    }
    if (!filtered.length) {
      const tr = document.createElement('tr'), td = document.createElement('td'); td.colSpan = 6; td.className = 'hint';
      td.textContent = authenticated ? (query ? '没有匹配的数据集或版本' : '暂无 3D Tiles 缓存') : '请先登录管理员账户';
      tr.append(td); el('objectsRows').append(tr);
    }
    const total = filtered.reduce((v, r) => ({files:v.files+r.objects, bytes:v.bytes+r.bytes, hits:v.hits+r.hits}), {files:0,bytes:0,hits:0});
    el('objectsSummary').textContent = `${new Set(filtered.map(r=>r.dataset)).size} 个数据集 · ${filtered.length} 个版本 · ${total.files.toLocaleString()} 个文件 · ${size(total.bytes)} · 命中 ${total.hits.toLocaleString()} 次`;
    el('objectsPage').textContent = filtered.length ? `第 ${page}/${pages} 页 · 共 ${filtered.length} 个版本` : '0 条';
    el('objectsPrev').disabled = page <= 1; el('objectsNext').disabled = page >= pages;
  }
  async function load() {
    const seq = ++sequence;
    if (!authenticated) {
      rows = []; render();
      el('objectsStatus').textContent = '请登录管理员账户查看缓存';
      if (dialog.open && !deleting) dialog.close();
      return;
    }
    if (el('objects3d').hidden) return;
    el('objectsRefresh').disabled = true;
    el('objectsStatus').textContent = '正在读取缓存统计…';
    try {
      const response = await request('/api/v1/admin/objects3d', {cache:'no-store'});
      if (seq !== sequence) return;
      if (response.status === 401) { setAuth(false); return; }
      if (!response.ok) throw Error(`HTTP ${response.status}`);
      const data = await response.json();
      if (seq !== sequence) return;
      rows = data.datasets; render();
      el('objectsStatus').textContent = `更新于 ${new Date().toLocaleTimeString()}`;
    } catch (error) {
      if (seq !== sequence) return;
      rows = []; render(); el('objectsStatus').textContent = `读取失败 · ${error.message}，请刷新重试`;
    } finally { if (seq === sequence || !authenticated) el('objectsRefresh').disabled = false; }
  }
  window.refreshObjects3d = load;
  el('objectsRefresh').onclick = load;
  el('objectsSearch').oninput = () => { page = 1; render(); };
  el('objectsPrev').onclick = () => { page--; render(); };
  el('objectsNext').onclick = () => { page++; render(); };
  el('objectsCancel').onclick = () => dialog.close();
  dialog.addEventListener('cancel', e => { if (deleting) e.preventDefault(); });
  dialog.addEventListener('close', () => { target = null; });
  el('objectsDelete').onclick = async () => {
    if (!target || deleting) return;
    const selected = target;
    deleting = true; el('objectsDelete').disabled = el('objectsCancel').disabled = true;
    el('objectsConfirmStatus').textContent = '正在清理…';
    try {
      const response = await request(`/api/v1/admin/objects3d/${encodeURIComponent(selected.dataset)}/${encodeURIComponent(selected.revision)}`, {method:'DELETE'});
      if (response.status === 401) { dialog.close(); setAuth(false); throw Error('登录已过期，请重新登录'); }
      if (!response.ok) throw Error(`HTTP ${response.status}`);
      const data = await response.json(); dialog.close();
      await load();
      el('objectsStatus').textContent = `已清理 ${selected.dataset} 的 ${data.deleted_objects} 个文件（${size(data.deleted_bytes)}）；源数据保留，访问时自动回填。`;
    } catch (error) { el(dialog.open ? 'objectsConfirmStatus' : 'objectsStatus').textContent = `清理失败 · ${error.message}`; }
    finally { deleting = false; el('objectsDelete').disabled = el('objectsCancel').disabled = false; }
  };
  render();
})();
