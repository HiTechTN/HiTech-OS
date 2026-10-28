/* ORION-X Spaceship Cockpit — front-end controller.
 * The Rust core owns all privileged operations; this file only renders state
 * and sends explicit operator intent to /api/*.
 */

type Lang = "en" | "fr" | "ar";
type Mode = "replace" | "erase";

interface Partition {
  name: string;
  path: string;
  size_bytes: number;
  size_human: string;
  fstype: string;
  mountpoint: string;
  label: string;
  partlabel: string;
  uuid: string;
  parttype: string;
  is_efi: boolean;
}
interface Disk {
  name: string;
  path: string;
  size_bytes: number;
  size_human: string;
  model: string;
  partitions: Partition[];
}
interface WifiNetwork { ssid: string; signal: string; security: string; }
interface Status {
  running: boolean;
  success: boolean;
  progress: number;
  status: string;
  logs: string;
  network_online: boolean;
}

const I18N: Record<Lang, Record<string, string>> = {
  en: {
    terminal: "Terminal", reboot: "Reboot", systemOnline: "System online",
    brandTag: "HITECH-OS // INSTALLATION VESSEL",
    welcomeKicker: "FLIGHT CONTROL // INSTALLER READY",
    welcomeTitle: "Chart a new system.",
    welcomeBody: "This vessel installs HiTech-OS with your identity, desktop, and storage layout. Every control is live. Every choice is reversible until ignition.",
    begin: "Begin pre-flight checks", telemetry: "Telemetry nominal",
    identity: "Crew identity", hostname: "Hostname", username: "Username", password: "Password",
    identityHelp: "The password is written once to the target configuration as a hashed NixOS password.",
    desktop: "Bridge configuration", storage: "Target storage",
    modeReplace: "Safe dual-boot", modeErase: "Erase disk",
    modeReplaceHint: "Choose an existing root partition and its EFI partition. Other partitions are never formatted.",
    modeEraseHint: "The selected disk is erased completely: GPT, 512 MiB EFI, and ext4 root.",
    refresh: "Refresh disks", openGparted: "Open GParted",
    network: "Uplink / Wi-Fi", checkingNetwork: "Scanning spectrum…", scanWifi: "Scan Wi-Fi",
    wifiPassword: "Wi-Fi password", connect: "Connect",
    readyToInstall: "All systems checked. Ignition writes the selected configuration.",
    install: "Ignite installation", installingKicker: "ORION-X // IGNITION SEQUENCE",
    installing: "Installing HiTech-OS", showLogs: "Show telemetry", hideLogs: "Hide telemetry",
    rebootNow: "Reboot now", selectRoot: "Root partition", selectEfi: "EFI partition",
    noDisks: "No installable disks detected. Refresh or open GParted.", selectDisk: "Select a target disk first.",
    efiMissing: "No EFI system partition found on this disk.", selectRootFirst: "Select a root partition.",
    selectEfiFirst: "Select an EFI partition.", passwordRequired: "Enter a password (4+ characters).",
    hostnameInvalid: "Hostname must contain only letters, numbers, and hyphens.",
    scanning: "Scanning Wi-Fi spectrum…", noWifi: "No networks found. Try again.",
    wifiConnected: "Uplink established.", gpartedStarted: "GParted launched.",
    terminalOpened: "Terminal opened.", refreshDone: "Disk telemetry refreshed.",
    installDone: "Installation complete. The target is ready for reboot.",
    installFailed: "Installation stopped. Review telemetry and retry.",
    showLogsHint: "Show telemetry", hideLogsHint: "Hide telemetry",
    desktop_orion_name: "Orion", desktop_orion_desc: "HiTech cockpit shell",
    desktop_plasma6_name: "KDE Plasma 6", desktop_plasma6_desc: "Modern Qt workspace",
    desktop_gnome_name: "GNOME", desktop_gnome_desc: "Clean GNOME desktop",
    desktop_xfce_name: "XFCE", desktop_xfce_desc: "Lightweight classic desktop",
    dualboot: "Safe dual-boot", erased: "Erase disk", selected: "Selected"
  },
  fr: {
    terminal: "Terminal", reboot: "Redémarrer", systemOnline: "Système en ligne",
    brandTag: "HITECH-OS // VAISSEAU D'INSTALLATION",
    welcomeKicker: "CONTRÔLE DE VOL // INSTALLATEUR PRÊT",
    welcomeTitle: "Tracez un nouveau système.",
    welcomeBody: "Ce vaisseau installe HiTech-OS avec votre identité, votre bureau et votre stockage. Chaque contrôle est actif. Chaque choix reste réversible jusqu'à l'allumage.",
    begin: "Lancer les vérifications", telemetry: "Télémétrie nominale",
    identity: "Identité de l'équipage", hostname: "Nom d'hôte", username: "Utilisateur", password: "Mot de passe",
    identityHelp: "Le mot de passe est écrit une seule fois dans la configuration cible, haché par NixOS.",
    desktop: "Configuration du pont", storage: "Stockage cible",
    modeReplace: "Double démarrage sûr", modeErase: "Effacer le disque",
    modeReplaceHint: "Choisissez une partition racine existante et sa partition EFI. Les autres partitions ne sont jamais formatées.",
    modeEraseHint: "Le disque sélectionné est entièrement effacé : GPT, EFI 512 Mio et racine ext4.",
    refresh: "Actualiser les disques", openGparted: "Ouvrir GParted",
    network: "Liaison / Wi-Fi", checkingNetwork: "Analyse du spectre…", scanWifi: "Scanner le Wi-Fi",
    wifiPassword: "Mot de passe Wi-Fi", connect: "Connecter",
    readyToInstall: "Tous les systèmes sont vérifiés. L'allumage écrit la configuration choisie.",
    install: "Allumer l'installation", installingKicker: "ORION-X // SÉQUENCE D'ALLUMAGE",
    installing: "Installation de HiTech-OS", showLogs: "Afficher la télémétrie", hideLogs: "Masquer la télémétrie",
    rebootNow: "Redémarrer", selectRoot: "Partition racine", selectEfi: "Partition EFI",
    noDisks: "Aucun disque installable. Actualisez ou ouvrez GParted.", selectDisk: "Sélectionnez d'abord un disque cible.",
    efiMissing: "Aucune partition système EFI sur ce disque.", selectRootFirst: "Sélectionnez une partition racine.",
    selectEfiFirst: "Sélectionnez une partition EFI.", passwordRequired: "Saisissez un mot de passe (4 caractères minimum).",
    hostnameInvalid: "Le nom d'hôte ne peut contenir que des lettres, chiffres et tirets.",
    scanning: "Analyse du spectre Wi-Fi…", noWifi: "Aucun réseau trouvé. Réessayez.",
    wifiConnected: "Liaison établie.", gpartedStarted: "GParted lancé.",
    terminalOpened: "Terminal ouvert.", refreshDone: "Télémétrie des disques actualisée.",
    installDone: "Installation terminée. La cible est prête à redémarrer.",
    installFailed: "Installation arrêtée. Consultez la télémétrie et réessayez.",
    showLogsHint: "Afficher la télémétrie", hideLogsHint: "Masquer la télémétrie",
    desktop_orion_name: "Orion", desktop_orion_desc: "Shell cockpit HiTech",
    desktop_plasma6_name: "KDE Plasma 6", desktop_plasma6_desc: "Espace de travail Qt moderne",
    desktop_gnome_name: "GNOME", desktop_gnome_desc: "Bureau GNOME épuré",
    desktop_xfce_name: "XFCE", desktop_xfce_desc: "Bureau classique léger",
    dualboot: "Double démarrage sûr", erased: "Effacer le disque", selected: "Sélectionné"
  },
  ar: {
    terminal: "الطرفية", reboot: "إعادة التشغيل", systemOnline: "النظام متصل",
    brandTag: "HITECH-OS // سفينة التثبيت",
    welcomeKicker: "مركز التحكم // المثبّت جاهز",
    welcomeTitle: "ارسم نظاماً جديداً.",
    welcomeBody: "تثبّت هذه السفينة HiTech-OS بهويتك وسطح مكتبك وتخطيط التخزين. كل عنصر تحكم نشط، وكل خيار قابل للتراجع حتى لحظة الإشعال.",
    begin: "بدء فحوصات ما قبل الإقلاع", telemetry: "القياسات طبيعية",
    identity: "هوية الطاقم", hostname: "اسم المضيف", username: "اسم المستخدم", password: "كلمة المرور",
    identityHelp: "تُكتب كلمة المرور مرة واحدة في إعدادات الهدف بصيغة NixOS المشفّرة.",
    desktop: "إعداد الجسر", storage: "التخزين الهدف",
    modeReplace: "إقلاع مزدوج آمن", modeErase: "مسح القرص",
    modeReplaceHint: "اختر قسم الجذر الموجود وقسم EFI الخاص به. لا تُهيّأ الأقسام الأخرى أبداً.",
    modeEraseHint: "يُمسح القرص المحدد بالكامل: GPT و EFI بحجم 512 MiB وجذر ext4.",
    refresh: "تحديث الأقراص", openGparted: "فتح GParted",
    network: "الاتصال / Wi-Fi", checkingNetwork: "جارٍ مسح الطيف…", scanWifi: "مسح شبكات Wi-Fi",
    wifiPassword: "كلمة مرور Wi-Fi", connect: "اتصال",
    readyToInstall: "جميع الأنظمة جاهزة. الإشعال يكتب الإعدادات المختارة.",
    install: "إشعال التثبيت", installingKicker: "ORION-X // تسلسل الإشعال",
    installing: "جارٍ تثبيت HiTech-OS", showLogs: "إظهار القياسات", hideLogs: "إخفاء القياسات",
    rebootNow: "إعادة التشغيل الآن", selectRoot: "قسم الجذر", selectEfi: "قسم EFI",
    noDisks: "لم يُعثر على أقراص قابلة للتثبيت. حدّث أو افتح GParted.", selectDisk: "اختر قرصاً هدفاً أولاً.",
    efiMissing: "لا يوجد قسم نظام EFI على هذا القرص.", selectRootFirst: "اختر قسم الجذر.",
    selectEfiFirst: "اختر قسم EFI.", passwordRequired: "أدخل كلمة مرور (4 أحرف على الأقل).",
    hostnameInvalid: "اسم المضيف يقبل الأحرف والأرقام والشرطات فقط.",
    scanning: "جارٍ مسح طيف Wi-Fi…", noWifi: "لم تُعثر على شبكات. حاول مرة أخرى.",
    wifiConnected: "تم إنشاء الاتصال.", gpartedStarted: "تم تشغيل GParted.",
    terminalOpened: "تم فتح الطرفية.", refreshDone: "تم تحديث قياسات الأقراص.",
    installDone: "اكتمل التثبيت. الهدف جاهز لإعادة التشغيل.",
    installFailed: "توقف التثبيت. راجع القياسات وأعد المحاولة.",
    showLogsHint: "إظهار القياسات", hideLogsHint: "إخفاء القياسات",
    desktop_orion_name: "Orion", desktop_orion_desc: "واجهة cockpit من HiTech",
    desktop_plasma6_name: "KDE Plasma 6", desktop_plasma6_desc: "مساحة عمل Qt حديثة",
    desktop_gnome_name: "GNOME", desktop_gnome_desc: "سطح مكتب GNOME أنيق",
    desktop_xfce_name: "XFCE", desktop_xfce_desc: "سطح مكتب كلاسيكي خفيف",
    dualboot: "إقلاع مزدوج آمن", erased: "مسح القرص", selected: "محدد"
  }
};

// The kiosk browser may open its first window before the compositor grants
// fullscreen. Capture the first pointer gesture (the operator's click on the
// welcome screen) and promote the document to fullscreen synchronously; the
// click itself still reaches the button underneath.
let cockpitFullscreenRequested = false;
document.addEventListener("pointerdown", () => {
  if (cockpitFullscreenRequested) return;
  cockpitFullscreenRequested = true;
  if (!document.fullscreenElement) {
    document.documentElement.requestFullscreen?.().catch(() => {});
  }
}, { capture: true, once: true });

const DESKTOPS = [
  { id: "orion", glyph: "◈" },
  { id: "plasma6", glyph: "◍" },
  { id: "gnome", glyph: "⬡" },
  { id: "xfce", glyph: "▣" }
];

const byId = (id: string): HTMLElement => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el;
};
const t = (key: string): string => I18N[lang][key] ?? I18N.en[key] ?? key;
// Wrap technical text (device paths, sizes, fs types) in bidi isolates so an
// RTL layout keeps "476.9 GiB" and "/dev/nvme0n1p3" in readable LTR order.
const isolated = (s: string): string => `\u2066${s}\u2069`;

const esc = (s: string): string => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c] as string));

let lang: Lang = (localStorage.getItem("hitech-lang") as Lang) || "en";
if (!I18N[lang]) lang = "en";
let mode: Mode = "replace";
let desktop = "orion";
let disks: Disk[] = [];
let selectedDisk = "";
let rootPartition = "";
let efiPartition = "";
let selectedWifi = "";
let networks: WifiNetwork[] = [];
let pollTimer = 0;
let logsVisible = false;

const clock = (): void => {
  const el = byId("missionClock");
  const now = new Date();
  el.textContent = now.toLocaleTimeString("en-GB", { hour12: false });
};

function applyLanguage(): void {
  localStorage.setItem("hitech-lang", lang);
  document.documentElement.lang = lang;
  document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n || "");
  });
  document.querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder || "");
  });
  document.querySelectorAll<HTMLElement>(".lang").forEach((el) => {
    el.classList.toggle("active", el.dataset.lang === lang);
  });
  renderDesktops();
  renderStorage();
  updateModeHint();
  updateLaunchSummary();
  const toggle = byId("toggleLog") as HTMLButtonElement;
  toggle.textContent = t(logsVisible ? "hideLogs" : "showLogs");
  document.querySelectorAll(".mode").forEach((el) => {
    const m = (el as HTMLElement).dataset.mode;
    if (m === mode) el.classList.add("active"); else el.classList.remove("active");
  });
}

function toast(message: string): void {
  const el = byId("toast");
  el.textContent = message;
  el.classList.add("show");
  window.setTimeout(() => el.classList.remove("show"), 4200);
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, init);
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

function setNetwork(online: boolean, text?: string): void {
  const el = byId("networkStatus");
  const key = online ? "systemOnline" : "noWifi";
  el.innerHTML = `<span class="pulse" style="background:${online ? "var(--emerald)" : "var(--red)"};box-shadow:0 0 12px ${online ? "var(--emerald)" : "var(--red)"}"></span><span ${text ? "" : `data-i18n="${key}"`}>${esc(text || t(key))}</span>`;
}

function updateModeHint(): void {
  byId("modeHint").textContent = t(mode === "replace" ? "modeReplaceHint" : "modeEraseHint");
  document.querySelectorAll(".mode").forEach((el) => {
    const m = (el as HTMLElement).dataset.mode;
    el.classList.toggle("active", m === mode);
  });
}

function updateLaunchSummary(): void {
  const disk = disks.find((d) => d.path === selectedDisk);
  const label = mode === "replace" ? t("dualboot") : t("erased");
  byId("launchSummary").textContent = `${t(`desktop_${desktop}_name`)} // ${label}${disk ? ` // ${disk.name} ${disk.size_human}` : ""}`;
}

function currentDisk(): Disk | undefined {
  return disks.find((d) => d.path === selectedDisk);
}

function partitionClass(p: Partition): string {
  if (p.path === rootPartition) return "partition root";
  if (p.path === efiPartition && p.is_efi) return "partition efi";
  if (p.fstype === "ntfs" || p.fstype === "bitlocker") return "partition windows";
  if (p.is_efi) return "partition efi";
  return "partition";
}

function renderStorage(): void {
  const list = byId("diskList");
  const detail = byId("diskDetail");
  const usable = disks.filter((d) => d.size_bytes >= 1024 * 1024 * 1024);
  if (!usable.length) {
    list.innerHTML = `<p class="hint">${esc(t("noDisks"))}</p>`;
    detail.innerHTML = "";
    return;
  }
  if (!selectedDisk || !usable.some((d) => d.path === selectedDisk)) {
    selectedDisk = usable[0].path;
  }
  list.innerHTML = usable.map((d) => {
    const parts = d.partitions || [];
    const total = parts.reduce((n, p) => n + Math.max(p.size_bytes, 1), 0) || 1;
    const bar = parts.length
      ? parts.map((p) => {
          const pct = Math.max((Math.max(p.size_bytes, 1) / total) * 100, 2.5);
          const label = p.name + (p.size_human ? ` ${p.size_human}` : "");
          return `<button class="${partitionClass(p)}" style="width:${pct}%" title="${esc(label)}" data-part="${esc(p.path)}">${esc(p.name)}</button>`;
        }).join("")
      : `<span class="partition empty" style="width:100%">${esc(t("noDisks"))}</span>`;
    return `<div class="disk-card ${d.path === selectedDisk ? "selected" : ""}" data-disk="${esc(d.path)}">
      <div class="disk-title"><span class="tech">${esc(d.path)}</span><span class="tech">${esc(d.size_human)}</span></div>
      <div class="disk-meta">${esc(d.model || "disk")}</div>
      <div class="partition-bar">${bar}</div>
    </div>`;
  }).join("");
  list.querySelectorAll<HTMLElement>("[data-disk]").forEach((el) => {
    el.addEventListener("click", () => {
      selectedDisk = el.dataset.disk || "";
      rootPartition = "";
      efiPartition = "";
      renderStorage();
    });
  });
  list.querySelectorAll<HTMLElement>("[data-part]").forEach((el) => {
    el.addEventListener("click", (ev) => {
      ev.stopPropagation();
      setRoot(el.dataset.part || "");
    });
  });
  const d = currentDisk();
  if (!d) { detail.innerHTML = ""; return; }
  if (mode === "erase") {
    detail.innerHTML = `<div class="warning">${esc(t("modeEraseHint"))}<br><strong>${esc(d.path)} — ${esc(d.size_human)}</strong></div>`;
    updateLaunchSummary();
    return;
  }
  const parts = (d.partitions || []).filter((p) => p.size_bytes > 0);
  const efiParts = parts.filter((p) => p.is_efi);
  if (!efiPartition && efiParts.length) efiPartition = efiParts[0].path;
  // Never offer the EFI system partition as a root filesystem: Mode A must
  // preserve bootloader files and the Windows/other-OS ESP.
  const rootParts = parts.filter((p) => !p.is_efi);
  const rootOptions = rootParts.map((p) => `<option value="${esc(p.path)}" ${p.path === rootPartition ? "selected" : ""}>${esc(isolated(`${p.path} — ${p.fstype || "raw"} — ${p.size_human}${p.mountpoint ? ` @ ${p.mountpoint}` : ""}`))}</option>`).join("");
  const efiOptions = efiParts.map((p) => `<option value="${esc(p.path)}" ${p.path === efiPartition ? "selected" : ""}>${esc(isolated(`${p.path} — ${p.fstype || "vfat"} — ${p.size_human}`))}</option>`).join("");
  detail.innerHTML = `
    <div class="detail-row"><span>${esc(t("selectRoot"))}</span><select id="rootSelect"><option value="">— ${esc(t("selectRoot"))} —</option>${rootOptions}</select></div>
    <div class="detail-row"><span>${esc(t("selectEfi"))}</span><select id="efiSelect"><option value="">— ${esc(t("selectEfi"))} —</option>${efiOptions}</select></div>
    ${efiParts.length ? "" : `<div class="warning">${esc(t("efiMissing"))}</div>`}
  `;
  const rootSelect = document.getElementById("rootSelect") as HTMLSelectElement | null;
  const efiSelect = document.getElementById("efiSelect") as HTMLSelectElement | null;
  rootSelect?.addEventListener("change", () => { rootPartition = rootSelect.value; renderStorage(); });
  efiSelect?.addEventListener("change", () => { efiPartition = efiSelect.value; renderStorage(); });
  updateLaunchSummary();
}

function setRoot(path: string): void {
  if (!path) return;
  const p = currentDisk()?.partitions.find((x) => x.path === path);
  if (!p) return;
  rootPartition = path;
  if (!efiPartition) {
    const efi = currentDisk()?.partitions.find((x) => x.is_efi);
    if (efi) efiPartition = efi.path;
  }
  renderStorage();
}

function renderDesktops(): void {
  const grid = byId("desktopGrid");
  grid.innerHTML = DESKTOPS.map((d) => `<button class="desktop-card ${d.id === desktop ? "selected" : ""}" data-desktop="${d.id}">
    <span class="desktop-glyph">${d.glyph}</span>
    <span><span class="desktop-name">${esc(t(`desktop_${d.id}_name`))}</span><span class="desktop-desc">${esc(t(`desktop_${d.id}_desc`))}</span></span>
  </button>`).join("");
  grid.querySelectorAll<HTMLElement>("[data-desktop]").forEach((el) => {
    el.addEventListener("click", () => {
      desktop = el.dataset.desktop || "orion";
      renderDesktops();
      updateLaunchSummary();
    });
  });
}

function renderWifi(): void {
  const list = byId("wifiList");
  if (!networks.length) { list.innerHTML = `<p class="hint">${esc(t("noWifi"))}</p>`; return; }
  list.innerHTML = networks.map((n) => `<button class="wifi ${n.ssid === selectedWifi ? "selected" : ""}" data-ssid="${esc(n.ssid)}">
    <span>${esc(n.ssid)}</span><small>${esc(n.signal || "")} ${n.security && n.security !== "--" ? "🔒" : ""}</small>
  </button>`).join("");
  list.querySelectorAll<HTMLElement>("[data-ssid]").forEach((el) => {
    el.addEventListener("click", () => { selectedWifi = el.dataset.ssid || ""; renderWifi(); });
  });
}

async function loadDisks(showToast = false): Promise<void> {
  try {
    const data = await api<{ disks: Disk[] }>("/api/disks");
    disks = (data.disks || []).filter((d) => d.size_bytes >= 1024 * 1024 * 1024);
    if (selectedDisk && !disks.some((d) => d.path === selectedDisk)) {
      selectedDisk = disks[0]?.path || "";
      rootPartition = "";
      efiPartition = "";
    }
    renderStorage();
    if (showToast) toast(t("refreshDone"));
  } catch (e) {
    toast(String(e));
  }
}

async function scanWifi(): Promise<void> {
  const radar = document.querySelector(".radar");
  radar?.classList.add("sweeping");
  setNetwork(false, t("scanning"));
  try {
    const data = await api<{ networks: WifiNetwork[] }>("/api/wifi/scan", { method: "POST" });
    networks = data.networks || [];
    renderWifi();
    setNetwork(networks.length > 0, networks.length ? t("telemetry") : t("noWifi"));
  } catch (e) {
    setNetwork(false, String(e));
  } finally {
    radar?.classList.remove("sweeping");
  }
}

async function connectWifi(): Promise<void> {
  if (!selectedWifi) { toast(t("noWifi")); return; }
  const password = (byId("wifiPassword") as HTMLInputElement).value;
  try {
    await api("/api/wifi/connect", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ ssid: selectedWifi, password }).toString()
    });
    setNetwork(true, t("wifiConnected"));
    toast(t("wifiConnected"));
  } catch (e) { toast(String(e)); }
}

async function startInstall(): Promise<void> {
  const hostname = (byId("hostname") as HTMLInputElement).value.trim();
  const username = (byId("username") as HTMLInputElement).value.trim();
  const password = (byId("password") as HTMLInputElement).value;
  if (!/^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$/.test(hostname)) { toast(t("hostnameInvalid")); return; }
  if (!/^[a-z_][a-z0-9_-]{0,31}$/.test(username)) { toast(t("hostnameInvalid")); return; }
  if (password.length < 4) { toast(t("passwordRequired")); return; }
  if (mode === "replace") {
    if (!rootPartition) { toast(t("selectRootFirst")); return; }
    if (!efiPartition) { toast(t("selectEfiFirst")); return; }
  } else if (!selectedDisk) { toast(t("selectDisk")); return; }

  const overlay = byId("installOverlay");
  overlay.hidden = false;
  byId("installTitle").textContent = t("installing");
  byId("installStatus").textContent = t("checkingNetwork");
  byId("progressFill").style.width = "0%";
  byId("progressText").textContent = "0%";
  byId("finishBtn").hidden = true;
  logsVisible = false;
  byId("installLog").classList.remove("visible");
  byId("toggleLog").textContent = t("showLogs");

  const body = new URLSearchParams({
    mode, hostname, username, password, desktop,
    disk: selectedDisk, root_partition: rootPartition, efi_partition: efiPartition
  }).toString();
  try {
    const res = await fetch("/api/install", {
      method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body
    });
    if (!res.ok) throw new Error(await res.text());
    pollStatus();
  } catch (e) {
    byId("installStatus").textContent = `${t("installFailed")}: ${String(e)}`;
  }
}

async function pollStatus(): Promise<void> {
  window.clearTimeout(pollTimer);
  try {
    const s = await api<Status>("/api/status");
    const pct = Math.max(0, Math.min(100, s.progress || 0));
    byId("progressFill").style.width = `${pct}%`;
    byId("progressText").textContent = `${pct}%`;
    byId("installStatus").textContent = s.status || "";
    byId("installLog").textContent = s.logs || "";
    setNetwork(s.network_online);
    if (!s.running) {
      if (s.success) {
        byId("installTitle").textContent = t("installDone");
        byId("finishBtn").hidden = false;
      } else {
        byId("installTitle").textContent = t("installFailed");
      }
      return;
    }
    pollTimer = window.setTimeout(pollStatus, 800);
  } catch (e) {
    byId("installStatus").textContent = String(e);
    pollTimer = window.setTimeout(pollStatus, 1800);
  }
}

async function action(path: string, message: string): Promise<void> {
  try {
    await api(path, { method: "POST" });
    toast(message);
  } catch (e) { toast(String(e)); }
}

function init(): void {
  applyLanguage();
  clock();
  window.setInterval(clock, 1000);

  document.querySelectorAll<HTMLElement>(".lang").forEach((el) => {
    el.addEventListener("click", () => {
      lang = (el.dataset.lang as Lang) || "en";
      applyLanguage();
    });
  });
  document.querySelectorAll<HTMLElement>(".mode").forEach((el) => {
    el.addEventListener("click", () => {
      mode = (el.dataset.mode as Mode) || "replace";
      updateModeHint();
      renderStorage();
    });
  });
  byId("beginBtn").addEventListener("click", () => {
    byId("hero").hidden = true;
    byId("config").hidden = false;
    window.scrollTo({ top: 0, behavior: "smooth" });
  });
  byId("refreshDisks").addEventListener("click", () => loadDisks(true));
  byId("gpartedBtn").addEventListener("click", () => action("/api/gparted", t("gpartedStarted")));
  byId("terminalBtn").addEventListener("click", () => action("/api/terminal", t("terminalOpened")));
  byId("rebootBtn").addEventListener("click", () => {
    if (window.confirm(t("reboot") + "?")) action("/api/reboot", t("rebootNow"));
  });
  byId("scanWifi").addEventListener("click", scanWifi);
  byId("connectWifi").addEventListener("click", connectWifi);
  byId("installBtn").addEventListener("click", startInstall);
  byId("toggleLog").addEventListener("click", () => {
    logsVisible = !logsVisible;
    byId("installLog").classList.toggle("visible", logsVisible);
    byId("toggleLog").textContent = t(logsVisible ? "hideLogs" : "showLogs");
  });
  byId("finishBtn").addEventListener("click", () => action("/api/reboot", t("rebootNow")));

  loadDisks();
  api<{ online: boolean }>("/api/network").then((n) => setNetwork(Boolean(n.online))).catch(() => setNetwork(false));
}

init();
