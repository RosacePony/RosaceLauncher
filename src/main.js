const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const state = { titles: [], releases: {}, progress: {}, errors: {}, selected: null };
// Installed titles update themselves when a newer GitHub release is found (setting kept per PC).
const autoUpdate = () => localStorage.getItem("autoUpdate") !== "off";
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const mb = (n) => (n / 1048576).toFixed(1) + " MB";

function status(t) {
  const rel = state.releases[t.id];
  if (t.store_url) return "store";
  if (state.progress[t.id]) return "busy";
  if (!t.supported) return "unsupported";
  if (!t.installed) return rel && !rel.error ? "available" : "unavailable";
  if (rel && !rel.error && rel.tag !== t.installed.version) return "update";
  return "installed";
}

function renderList() {
  $("list").innerHTML = state.titles.map((t) => {
    const s = status(t);
    const badge = { update: '<span class="badge update">Update</span>', installed: '<span class="badge installed">Installed</span>',
      busy: '<span class="badge">…</span>', store: '<span class="badge">Steam</span>' }[s] || "";
    return `<button class="item ${t.id === state.selected ? "active" : ""}" data-id="${esc(t.id)}">${esc(t.name)}${badge}</button>`;
  }).join("") || '<p class="muted">No titles in catalog.</p>';
  document.querySelectorAll(".item").forEach((b) => (b.onclick = () => { state.selected = b.dataset.id; render(); }));
}

function renderDetail() {
  const t = state.titles.find((x) => x.id === state.selected);
  if (!t) { $("detail").innerHTML = '<p class="muted">Select a title.</p>'; return; }
  const s = status(t), rel = state.releases[t.id], p = state.progress[t.id], err = state.errors[t.id];
  let actions = "";
  if (s === "store") {
    actions = `<button class="primary" data-act="store">View on Steam</button><span class="meta">Available on Steam</span>`;
  } else if (s === "busy") {
    const pct = p.total ? Math.round((p.downloaded / p.total) * 100) : 0;
    const label = p.stage === "extract" ? "Installing…" : `Downloading ${mb(p.downloaded)} / ${mb(p.total)} (${pct}%)`;
    actions = `<div style="flex:1"><div class="progress"><div style="width:${p.stage === "extract" ? 100 : pct}%"></div></div><span class="meta">${label}</span></div>`;
  } else if (s === "installed" || s === "update") {
    actions = `<button class="primary" data-act="launch">Play</button>` +
      (s === "update" ? `<button class="secondary" data-act="install">Update to ${esc(rel.tag)}</button>` : "") +
      `<button class="secondary" data-act="folder">Open folder</button><button class="ghost" data-act="uninstall">Uninstall</button>`;
  } else if (s === "available") {
    actions = `<button class="primary" data-act="install">Install</button><span class="meta">${mb(rel.size)}</span>`;
  } else if (s === "unsupported") {
    actions = `<span class="meta">Not available on this system yet.</span>`;
  } else {
    actions = `<span class="meta">${rel ? esc(rel.error) : "Checking for releases…"}</span>`;
  }
  const hero = t.cover ? ` style="background-image:url('${esc(t.cover)}')"` : "";
  $("detail").innerHTML = `
    <div class="hero"${hero}><h1>${esc(t.name)}</h1></div>
    <div class="body">
      <div class="actions">${actions}</div>
      ${err ? `<p class="error">${esc(err)}</p>` : ""}
      <p>${esc(t.description)}</p>
      <p class="meta">${t.installed ? `Installed: ${esc(t.installed.version)} · ` : ""}${rel && !rel.error ? `Latest: ${esc(rel.name)}${rel.published_at ? " · " + rel.published_at.slice(0, 10) : ""}` : ""}</p>
      ${rel && rel.notes ? `<h3>Patch notes</h3><div class="notes">${esc(rel.notes)}</div>` : ""}
    </div>`;
  document.querySelectorAll("[data-act]").forEach((b) => (b.onclick = () => act(t, b.dataset.act)));
}

function render() { renderList(); renderDetail(); }

async function act(t, action) {
  delete state.errors[t.id];
  try {
    if (action === "install") {
      state.progress[t.id] = { stage: "download", downloaded: 0, total: state.releases[t.id]?.size || 0 };
      render();
      t.installed = await invoke("install_title", { id: t.id });
    } else if (action === "launch") {
      // Never start an outdated build: update first, then play.
      if (status(t) === "update") {
        state.progress[t.id] = { stage: "download", downloaded: 0, total: state.releases[t.id]?.size || 0 };
        render();
        t.installed = await invoke("install_title", { id: t.id });
      }
      await invoke("launch_title", { id: t.id });
    } else if (action === "store") {
      await invoke("open_link", { url: t.store_url });
    } else if (action === "folder") {
      await invoke("open_install_folder", { id: t.id });
    } else if (action === "uninstall") {
      if (!confirm(`Uninstall ${t.name}?`)) return;
      await invoke("uninstall_title", { id: t.id });
      t.installed = null;
    }
  } catch (e) {
    state.errors[t.id] = String(e);
  } finally {
    delete state.progress[t.id];
    render();
  }
}

async function load() {
  try {
    state.titles = await invoke("list_titles");
  } catch (e) {
    $("detail").innerHTML = `<p class="muted error">${esc(e)}</p>`;
    return;
  }
  if (!state.titles.some((t) => t.id === state.selected)) state.selected = state.titles[0]?.id ?? null;
  state.releases = {};
  render();
  await Promise.all(state.titles.filter((t) => t.supported && !t.store_url).map(async (t) => {
    try { state.releases[t.id] = await invoke("latest_release", { id: t.id }); }
    catch (e) { state.releases[t.id] = { error: String(e) }; }
    render();
  }));
  if (autoUpdate()) {
    for (const t of state.titles.filter((x) => status(x) === "update")) await act(t, "install");
  }
}

listen("install-progress", (ev) => {
  if (ev.payload.stage === "done") return;
  state.progress[ev.payload.id] = ev.payload;
  renderDetail();
  renderList();
});
$("refresh").onclick = load;
$("kofi").onclick = () => invoke("open_link", { url: "https://ko-fi.com/rosacepony" }).catch(() => {});
$("auto-update").checked = autoUpdate();
$("auto-update").onchange = (e) => localStorage.setItem("autoUpdate", e.target.checked ? "on" : "off");
// Check for new releases every 30 minutes while the launcher stays open.
setInterval(() => { if (!Object.keys(state.progress).length) load(); }, 30 * 60 * 1000);
load();
