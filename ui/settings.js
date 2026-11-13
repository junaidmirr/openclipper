const { invoke } = window.__TAURI__?.core || {
  invoke: async (cmd, args) => console.log(`[Mock Invoke] ${cmd}`, args),
};
const { listen } = window.__TAURI__?.event || {
  listen: async (event, cb) => console.log(`[Mock Listen] ${event}`),
};

const ACCENT_COLORS = {
  orange: "#ff6600",
  blue: "#2563eb",
  green: "#10b981",
  purple: "#8b5cf6",
  red: "#ef4444",
};

let currentSettings = {
  manual_copy_shortcut: "Ctrl+Shift+C",
  manual_cut_shortcut: "Ctrl+Shift+X",
  manual_paste_shortcut: "Ctrl+Shift+V",
  toggle_hud_shortcut: "Cmd+Shift+Space",
  double_tap_enabled: true,
  double_tap_timeout_ms: 500,

  slot_count: 9,
  auto_paste_pop: true,
  auto_copy_direction: "fifo",
  auto_cut_fallback: true,
  persist_slots: false,
  play_sounds: true,

  theme: "light",
  accent_color: "orange",
  hud_opacity: 100,
  show_hud_on_copy: false,

  launch_at_startup: false,
};

// UI Elements
const tabs = document.querySelectorAll(".tab-btn");
const tabPanes = document.querySelectorAll(".tab-pane");
const toast = document.getElementById("settings-toast");

const doubleTapCheck = document.getElementById("double-tap-enabled");
const speedRange = document.getElementById("double-tap-speed");
const speedLabel = document.getElementById("speed-label");

const slotCountBtns = document.querySelectorAll(".pill-choice");
const pastePopTrue = document.getElementById("paste-pop-true");
const pastePopFalse = document.getElementById("paste-pop-false");
const dirFifo = document.getElementById("dir-fifo");
const dirLifo = document.getElementById("dir-lifo");
const autoCutFallback = document.getElementById("auto-cut-fallback");
const persistSlots = document.getElementById("persist-slots");
const playSounds = document.getElementById("play-sounds");

const themeCards = document.querySelectorAll(".theme-card");
const accentBtns = document.querySelectorAll(".accent-btn");
const showHudOnCopy = document.getElementById("show-hud-on-copy");
const hudPreview = document.getElementById("hud-preview");

const launchStartup = document.getElementById("launch-startup");
const axDot = document.getElementById("ax-dot");
const axText = document.getElementById("ax-text");
const btnFixAx = document.getElementById("btn-fix-ax");
const btnReset = document.getElementById("btn-reset");

const btnCancel = document.getElementById("btn-cancel");
const btnSave = document.getElementById("btn-save");

let toastTimer = null;
function showToast(msg) {
  if (!toast) return;
  toast.textContent = msg;
  toast.classList.add("visible");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toast.classList.remove("visible");
  }, 2200);
}

// Tab Switching
tabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    tabs.forEach((t) => t.classList.remove("active"));
    tabPanes.forEach((p) => p.classList.remove("active"));

    tab.classList.add("active");
    const target = document.getElementById(`tab-${tab.dataset.tab}`);
    if (target) target.classList.add("active");
  });
});

// Update Accent Color CSS Variables
function setAccent(colorName) {
  const hex = ACCENT_COLORS[colorName] || ACCENT_COLORS.orange;
  document.documentElement.style.setProperty("--accent", hex);
  document.documentElement.style.setProperty(
    "--accent-light",
    `rgba(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)}, 0.12)`
  );
}

// Update Theme
function setTheme(theme) {
  if (theme === "dark") {
    document.body.classList.add("theme-dark");
  } else if (theme === "light") {
    document.body.classList.remove("theme-dark");
  } else {
    // System
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    if (prefersDark) {
      document.body.classList.add("theme-dark");
    } else {
      document.body.classList.remove("theme-dark");
    }
  }
}

// Render Settings onto Form Controls
function populateUI(s) {
  currentSettings = { ...s };

  // 1. Shortcuts
  doubleTapCheck.checked = s.double_tap_enabled;
  speedRange.value = s.double_tap_timeout_ms;
  speedLabel.textContent = `${s.double_tap_timeout_ms} ms`;

  // 2. Slots
  slotCountBtns.forEach((btn) => {
    btn.classList.toggle("active", parseInt(btn.dataset.count, 10) === s.slot_count);
  });

  if (s.auto_paste_pop) {
    pastePopTrue.checked = true;
  } else {
    pastePopFalse.checked = true;
  }

  if (s.auto_copy_direction === "lifo") {
    dirLifo.checked = true;
  } else {
    dirFifo.checked = true;
  }

  autoCutFallback.checked = s.auto_cut_fallback;
  persistSlots.checked = s.persist_slots;
  playSounds.checked = s.play_sounds;

  // 3. Appearance
  themeCards.forEach((c) => {
    c.classList.toggle("active", c.dataset.theme === s.theme);
  });

  accentBtns.forEach((b) => {
    b.classList.toggle("active", b.dataset.accent === s.accent_color);
  });

  showHudOnCopy.checked = s.show_hud_on_copy;

  setTheme(s.theme);
  setAccent(s.accent_color);

  // 4. General
  launchStartup.checked = s.launch_at_startup;
}

// Gather settings from UI
function readUI() {
  let selectedSlotCount = currentSettings.slot_count;
  const activeCountBtn = document.querySelector(".pill-choice.active");
  if (activeCountBtn) {
    selectedSlotCount = parseInt(activeCountBtn.dataset.count, 10);
  }

  let selectedTheme = currentSettings.theme;
  const activeThemeBtn = document.querySelector(".theme-card.active");
  if (activeThemeBtn) {
    selectedTheme = activeThemeBtn.dataset.theme;
  }

  let selectedAccent = currentSettings.accent_color;
  const activeAccentBtn = document.querySelector(".accent-btn.active");
  if (activeAccentBtn) {
    selectedAccent = activeAccentBtn.dataset.accent;
  }

  return {
    ...currentSettings,
    double_tap_enabled: doubleTapCheck.checked,
    double_tap_timeout_ms: parseInt(speedRange.value, 10),

    slot_count: selectedSlotCount,
    auto_paste_pop: pastePopTrue.checked,
    auto_copy_direction: dirLifo.checked ? "lifo" : "fifo",
    auto_cut_fallback: autoCutFallback.checked,
    persist_slots: persistSlots.checked,
    play_sounds: playSounds.checked,

    theme: selectedTheme,
    accent_color: selectedAccent,
    show_hud_on_copy: showHudOnCopy.checked,

    launch_at_startup: launchStartup.checked,
  };
}

// Event Listeners for Interactive Controls
speedRange.addEventListener("input", (e) => {
  speedLabel.textContent = `${e.target.value} ms`;
});

slotCountBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    slotCountBtns.forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
  });
});

themeCards.forEach((card) => {
  card.addEventListener("click", () => {
    themeCards.forEach((c) => c.classList.remove("active"));
    card.classList.add("active");
    setTheme(card.dataset.theme);
  });
});

accentBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    accentBtns.forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    setAccent(btn.dataset.accent);
  });
});

// Check Accessibility
async function checkAx() {
  try {
    const trusted = await invoke("check_accessibility");
    if (trusted) {
      axDot.className = "status-dot active";
      axText.textContent = "Granted — Shortcuts & Key Swallowing active";
      btnFixAx.style.display = "none";
    } else {
      axDot.className = "status-dot";
      axText.textContent = "Permission Required";
      btnFixAx.style.display = "block";
    }
  } catch (err) {
    console.error("AX check error:", err);
  }
}

btnFixAx.addEventListener("click", async () => {
  await invoke("prompt_accessibility");
  showToast("Opening macOS Accessibility settings...");
  setTimeout(checkAx, 2000);
});

// Save Settings
btnSave.addEventListener("click", async () => {
  try {
    const updated = readUI();
    await invoke("update_settings", { newSettings: updated });
    currentSettings = updated;
    showToast("Preferences applied & saved successfully!");
  } catch (err) {
    showToast(`Error saving: ${err}`);
  }
});

// Close Settings Window
btnCancel.addEventListener("click", async () => {
  await invoke("close_settings_window");
});

window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    await invoke("close_settings_window");
  }
});

// Reset to Factory Defaults
btnReset.addEventListener("click", async () => {
  if (confirm("Are you sure you want to restore all settings to default?")) {
    try {
      const defaults = await invoke("reset_settings");
      populateUI(defaults);
      showToast("Restored factory defaults!");
    } catch (err) {
      showToast(`Reset error: ${err}`);
    }
  }
});

// Initialization
async function init() {
  try {
    const s = await invoke("get_settings");
    populateUI(s);
  } catch (err) {
    console.warn("Could not fetch settings:", err);
  }

  await checkAx();

  await listen("accessibility-status", (event) => {
    if (event.payload) {
      axDot.className = "status-dot active";
      axText.textContent = "Granted — Shortcuts & Key Swallowing active";
      btnFixAx.style.display = "none";
    } else {
      axDot.className = "status-dot";
      axText.textContent = "Permission Required";
      btnFixAx.style.display = "block";
    }
  });
}

init();
