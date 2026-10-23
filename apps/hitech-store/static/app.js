/* HiTech Drive Explorer */
let curPath = "", viewMode = "grid", isAdmin = false;
let lastList = {dirs:[],files:[]};
let pvPlaylist = [];
const $ = id => document.getElementById(id);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const KIND_ICON = {audio:"🎵",video:"🎬",image:"🖼️",model:"🧊",markdown:"📝",code:"💻",pdf:"📕",word:"📘",excel:"📗",powerpoint:"📙",html:"🌐",archive:"📦",other:"📄",dir:"📁"};

function toast(msg){ const t=$("toast"); t.textContent=msg; t.classList.remove("hidden"); setTimeout(()=>t.classList.add("hidden"),2600); }

async function apiErr(r){
  // Surface FastAPI's clean `detail` message instead of raw JSON.
  try{ const j = await r.clone().json(); if(j && j.detail) return String(j.detail).slice(0,160); }catch{}
  try{ const t = await r.text(); return t.slice(0,160); }catch{ return "Erreur "+r.status; }
}
async function api(path, opts={}){
  const r = await fetch(path, opts);
  if(!r.ok) throw new Error(await apiErr(r));
  return r;
}

async function refreshAuth(){
  let me = {admin:false, github_oauth:false};
  try{ me = await (await api("/api/auth/me")).json(); isAdmin = !!me.admin; }catch{ isAdmin=false; }
  $("adminBtn").textContent = isAdmin ? "Console Admin ✓" : "Administration";
  $("githubLogin").classList.toggle("hidden", !me.github_oauth);
  $("uploadBar").classList.toggle("hidden", !isAdmin);
  if(isAdmin){
    const folders = ["public","private","agents/genio","agents/opencode","agents/claude","agents/shared"];
    $("upFolder").innerHTML = folders.map(f=>`<option value="${f}">${f}</option>`).join("");
  }
}

async function load(path=""){
  curPath = path;
  const data = await (await api("/api/drive/list?path="+encodeURIComponent(path))).json();
  lastList = data;
  $("crumbs").innerHTML = data.breadcrumbs.map((c,i)=>
    (i?'<span class="sep">›</span>':"")+`<a onclick="load('${esc(c.path)}')">${esc(c.name)}</a>`).join("");
  const ex = $("explorer");
  ex.className = viewMode;
  let html = "";
  for(const d of data.dirs)
    html += `<div class="card"><span class="ficon">📁</span><div><div class="fname"><a style="color:var(--acc);cursor:pointer" onclick="load('${esc(d.path)}')">${esc(d.name)}</a></div><div class="factions"><button class="btn ghost" onclick="load('${esc(d.path)}')">Ouvrir</button>${isAdmin?`<button class="btn ghost" onclick='openShare(${JSON.stringify(d.path)},true)'>🔗 Partager</button>`:""}</div></div></div>`;
  for(const f of data.files){
    const dl = "/api/drive/download?path="+encodeURIComponent(f.path);
    html += `<div class="card"><span class="ficon">${KIND_ICON[f.kind]||"📄"}</span><div style="min-width:0;flex:1">
      <div class="fname">${esc(f.name)}</div>
      <div class="fmeta">${esc(f.size_str)} · ${esc(f.mtime)}${f.sha_short?`<br><code title="${esc(f.sha256)}">⛨ ${esc(f.sha_short)}</code>`:""}</div>
      <div class="factions"><a class="btn" href="${dl}" download>⬇ Télécharger</a>
      ${f.kind!=="other"?`<button class="btn ghost" onclick='preview(${JSON.stringify(f.path)},${JSON.stringify(f.name)},${JSON.stringify(f.kind)})'>👁 Aperçu</button>`:""}
      ${isAdmin?`<button class="btn ghost" onclick='openShare(${JSON.stringify(f.path)},false)'>🔗</button>`:""}</div>
    </div></div>`;
  }
  ex.innerHTML = html;
  $("empty").classList.toggle("hidden", data.dirs.length+data.files.length>0);
}

async function preview(path, name, kind){
  $("pvTitle").textContent = name;
  const body = $("pvBody"); body.innerHTML = "<p class='muted'>Chargement…</p>";
  $("previewModal").classList.remove("hidden");
  const url = "/api/drive/download?path="+encodeURIComponent(path);
  const open = `<p><a class="openlink" href="${url}" target="_blank" rel="noopener">↗ Ouvrir dans un nouvel onglet</a></p>`;
  const speedOpts = `<select onchange="pvSetSpeed(this)" title="Vitesse de lecture"><option value="0.75">0.75×</option><option value="1" selected>1×</option><option value="1.25">1.25×</option><option value="1.5">1.5×</option><option value="2">2×</option></select>`;
  try{
    if(kind==="audio"){
      pvPlaylist = (lastList.files||[]).filter(f=>f.kind==="audio");
      const idx = Math.max(0, pvPlaylist.findIndex(f=>f.path===path));
      const tracks = pvPlaylist.map((f,i)=>`<div data-p="${esc(f.path)}" class="${i===idx?'on':''}" onclick='pvPlayTrack(${JSON.stringify(f.path)},${JSON.stringify(f.name)})'>${i===idx?'▶ ':'🎵 '}${esc(f.name)}</div>`).join("");
      body.innerHTML = `<audio id="pvAudio" controls src="${url}"></audio>
        <div class="playerbar"><button class="btn ghost" onclick="pvStep(-1)">⏮</button><button class="btn ghost" onclick="pvSkip(-10)">−10s</button><button class="btn ghost" onclick="pvSkip(10)">+10s</button><button class="btn ghost" onclick="pvStep(1)">⏭</button>${speedOpts}</div>
        ${pvPlaylist.length>1?`<div id="pvTracks" class="tracklist">${tracks}</div>`:""}`+open;
    }
    else if(kind==="video") body.innerHTML = `<video id="pvVideo" controls src="${url}"></video>
      <div class="playerbar"><button class="btn ghost" onclick="pvVideoSkip(-10)">−10s</button><button class="btn ghost" onclick="pvVideoSkip(10)">+10s</button>${speedOpts}</div>`+open;
    else if(kind==="image") body.innerHTML = `<img src="${url}" alt="">`+open;
    else if(kind==="pdf") body.innerHTML = `<iframe class="docframe" src="${url}"></iframe>`+open;
    else if(kind==="html") body.innerHTML = `<iframe class="docframe" sandbox src="${url}"></iframe>`+open;
    else if(kind==="word"||kind==="excel"||kind==="powerpoint"){
      const pdfUrl = "/api/drive/office-pdf?path="+encodeURIComponent(path);
      body.innerHTML = `<iframe class="docframe" src="${pdfUrl}"></iframe>
        <p><a class="openlink" href="#" onclick='showExtract(${JSON.stringify(path)});return false;'>📝 Voir le texte extrait</a></p>
        <div id="pvExtract"></div>`+open;
    }
    else if(kind==="archive"){
      const d = await (await api("/api/drive/archive?path="+encodeURIComponent(path))).json();
      const list = d.entries.map(e=>`${e.size_str}  ${e.name}`).join("\n").slice(0,20000);
      body.innerHTML = `<p class='muted'>${d.count} fichier(s) dans l'archive</p><pre>${esc(list)||"—"}</pre>`+open;
    }
    else if(kind==="model") body.innerHTML = `<model-viewer src="${url}" camera-controls auto-rotate ar></model-viewer><p class="muted">Glisser pour pivoter · molette pour zoomer</p>`+open;
    else if(kind==="markdown"){
      const md = await (await api(url)).text();
      body.innerHTML = (window.marked ? `<div class="mdview">${marked.parse(md)}</div>` : `<pre>${esc(md)}</pre>`)+open;
    }
    else {
      const txt = await (await api(url)).text();
      body.innerHTML = `<pre><code id="pvCode" class="language-${codeLang(name)}">${esc(txt.slice(0,200000))}</code></pre>`+open;
      if(window.hljs){ try{ hljs.highlightElement($("pvCode")); }catch{} }
    }
  }catch(e){ body.innerHTML = `<p class="muted">Aperçu impossible : ${esc(e.message)}</p>`+open; }
}
function pvMedia(){ return $("pvAudio") || $("pvVideo"); }
function pvSetSpeed(sel){ const m = pvMedia(); if(m) m.playbackRate = parseFloat(sel.value); }
function pvSkip(s){ const a = $("pvAudio"); if(a) a.currentTime += s; }
function pvVideoSkip(s){ const v = $("pvVideo"); if(v) v.currentTime += s; }
function pvPlayTrack(path, name){
  const a = $("pvAudio"); if(!a) return;
  a.src = "/api/drive/download?path="+encodeURIComponent(path);
  a.play().catch(()=>{});
  document.querySelectorAll("#pvTracks div").forEach(d=>d.classList.toggle("on", d.dataset.p===path));
  $("pvTitle").textContent = name;
}
function pvStep(d){
  if(!pvPlaylist.length) return;
  let cur = null;
  try{ cur = new URL($("pvAudio").src).searchParams.get("path"); }catch{}
  let i = pvPlaylist.findIndex(f=>f.path===cur);
  i = (i+d+pvPlaylist.length)%pvPlaylist.length;
  pvPlayTrack(pvPlaylist[i].path, pvPlaylist[i].name);
}
function codeLang(name){
  const e = (String(name).split(".").pop()||"").toLowerCase();
  return {py:"python",js:"javascript",jsx:"javascript",ts:"typescript",tsx:"typescript",sh:"bash",json:"json",yaml:"yaml",yml:"yaml",toml:"ini",css:"css",xml:"xml",sql:"sql",c:"c",h:"c",cpp:"cpp",java:"java",go:"go",rs:"rust",php:"php",md:"markdown"}[e]||"plaintext";
}
async function showExtract(path){
  const box = $("pvExtract"); if(!box) return;
  box.innerHTML = "<p class='muted'>Extraction…</p>";
  try{
    const d = await (await api("/api/drive/extract?path="+encodeURIComponent(path))).json();
    box.innerHTML = d.html + (d.truncated?"<p class='muted'>…tronqué.</p>":"");
  }catch(e){ box.innerHTML = `<p class='muted'>Erreur: ${esc(e.message)}</p>`; }
}
let shareTarget = {path:"", dir:false};
function openShare(path, isDir){
  shareTarget = {path, dir:!!isDir};
  $("sharePath").textContent = (isDir?"📁 Dossier : ":"📄 Fichier : ") + path;
  $("shareOut").innerHTML = "";
  $("shareModal").classList.remove("hidden");
  loadShares();
}
function closeShare(){ $("shareModal").classList.add("hidden"); }
async function createShare(){
  $("shareOut").textContent = "Création…";
  try{
    const r = await (await api("/api/share/create",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:shareTarget.path,ttl_min:parseInt($("shareTtl").value),max_uses:parseInt($("shareUses").value)})})).json();
    $("shareOut").innerHTML = `<div class="sharelink">${esc(r.url)}</div>
      <button class="btn" onclick='navigator.clipboard.writeText(${JSON.stringify(r.url)}).then(()=>toast("Lien copié."))'>📋 Copier le lien</button>
      <p class="muted">Expire : ${esc((r.expires_at||"").slice(0,16).replace("T"," "))} · ${r.max_uses>0 ? (r.max_uses===1 ? "usage unique (1 seul téléchargement)" : r.max_uses+" téléchargements max") : "téléchargements illimités"}</p>`;
    loadShares();
  }catch(e){ $("shareOut").textContent = "Erreur: "+e.message; }
}
async function loadShares(){
  try{
    const d = await (await api("/api/share/list")).json();
    $("shareList").innerHTML = d.shares.length ? d.shares.map(s=>
      `<div class="sharerow"><span>${s.dir?"📁":"📄"} ${esc(s.name)} <span class="muted">(${s.uses}/${s.max_uses>0?s.max_uses:"∞"})</span></span><span style="white-space:nowrap"><button class="btn ghost" onclick='navigator.clipboard.writeText(${JSON.stringify(s.url)}).then(()=>toast("Lien copié."))'>📋</button> <button class="btn ghost" onclick='revokeShare(${JSON.stringify(s.id)})'>✕</button></span></div>`
    ).join("") : "Aucun lien actif.";
  }catch(e){ $("shareList").textContent = "Erreur: "+e.message; }
}
async function revokeShare(id){
  try{ await api("/api/share/revoke",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({id})}); loadShares(); toast("Lien révoqué."); }
  catch(e){ toast("Erreur: "+e.message); }
}
function closePreview(){ $("previewModal").classList.add("hidden"); $("pvBody").innerHTML=""; }

$("viewToggle").onclick = ()=>{ viewMode = viewMode==="grid"?"list":"grid"; $("viewToggle").textContent = viewMode==="grid"?"▦ Liste":"▦ Grille"; load(curPath); };
let verifying = false;
$("adminBtn").onclick = async ()=>{
  if(isAdmin){ location.href="/store/admin"; return; }
  // 1-click: open modal focused on PIN input, fire OTP in background
  $("pinInput").value = "";
  $("authModal").classList.remove("hidden");
  setTimeout(()=>$("pinInput").focus(), 60);
  $("authMsg").textContent = "Envoi du code en cours…";
  try{
    await api("/api/auth/quick-otp",{method:"POST"});
    $("authMsg").textContent = "Code PIN sécurisé envoyé par email (10 min, usage unique).";
  }catch(e){ $("authMsg").textContent = "Erreur: "+e.message; }
};
function closeAuth(){ $("authModal").classList.add("hidden"); }

async function quickOtp(){
  const b=$("otpBtn"); b.disabled=true; b.textContent="Envoi en cours…"; $("authMsg").textContent="";
  try{ await api("/api/auth/quick-otp",{method:"POST"}); $("authMsg").textContent="Code PIN sécurisé envoyé par email (10 min, usage unique)."; }
  catch(e){ $("authMsg").textContent="Erreur: "+e.message; }
  b.disabled=false; b.textContent="Renvoyer le code";
}
async function verifyPin(){
  if(verifying) return;
  const pin = $("pinInput").value.replace(/\D/g,"").slice(0,6);
  if(pin.length !== 6) return;
  verifying = true; $("authMsg").textContent="Vérification…";
  try{
    await api("/api/auth/verify-otp",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({pin})});
    closeAuth(); await refreshAuth(); await load(curPath); toast("Session admin active (12h).");
  }catch(e){ $("authMsg").textContent="Erreur: "+e.message; }
  verifying = false;
}
// Auto-submit as soon as 6 digits are typed
document.addEventListener("DOMContentLoaded", ()=>{
  $("pinInput").addEventListener("input", ()=>{
    $("pinInput").value = $("pinInput").value.replace(/\D/g,"").slice(0,6);
    if($("pinInput").value.length === 6) verifyPin();
  });
});
function uploadTarget(){ return curPath || $("upFolder").value || "public"; }
async function batchUpload(items){
  // items: [{file: File, rel: "sous/dossier/nom.ext"}] — arborescence conservée
  const fd = new FormData();
  fd.append("folder", uploadTarget());
  fd.append("paths", JSON.stringify(items.map(i=>i.rel)));
  for(const i of items) fd.append("files", i.file, i.file.name);
  $("upStatus").textContent = `Transfert de ${items.length} fichier(s)…`;
  try{
    const r = await (await api("/api/drive/upload-batch",{method:"POST",body:fd})).json();
    const fails = (r.files||[]).filter(f=>!f.ok);
    $("upStatus").textContent = fails.length
      ? `OK · ${r.uploaded}/${r.total} (échecs: ${fails.map(f=>f.path).join(", ").slice(0,120)})`
      : `OK · ${r.uploaded}/${r.total} fichier(s)`;
    load(curPath);
  }catch(e){ $("upStatus").textContent = "Erreur: "+e.message; }
}
async function uploadFile(){
  const files = [...$("upFile").files];
  if(!files.length){ toast("Choisis un ou plusieurs fichiers."); return; }
  await batchUpload(files.map(f=>({file:f, rel:f.name})));
  $("upFile").value = "";
}
function pickFolder(){ $("upDir").click(); }
$("upDir").addEventListener("change", async ()=>{
  const files = [...$("upDir").files];
  if(!files.length) return;
  await batchUpload(files.map(f=>({file:f, rel:f.webkitRelativePath||f.name})));
  $("upDir").value = "";
});
async function makeFolder(){
  const name = prompt("Nom du nouveau dossier :");
  if(!name || !name.trim()) return;
  $("upStatus").textContent = "Création…";
  try{
    const r = await (await api("/api/drive/mkdir",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({parent:curPath||"",name:name.trim()})})).json();
    $("upStatus").textContent = "Dossier créé.";
    await load(r.path);
  }catch(e){ $("upStatus").textContent = "Erreur: "+e.message; }
}

(async()=>{ await refreshAuth(); await load(""); })();
