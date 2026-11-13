// OpenClipper Micro HUD Engine

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

let slotCount = 9;
let slots = Array(slotCount).fill(null);
let autoCount = 0;
let currentMode = "idle"; // "idle" | "copy" | "cut" | "paste"
let selectedSlotIndex = null;
let settingsState = {
  play_sounds: true,
  theme: "light",
  accent_color: "orange",
  slot_count: 9,
};

const hudPill = document.getElementById("clipper-hud");
const dragHandle = document.getElementById("drag-handle");
const slotsContainer = document.getElementById("slots-container");
const modeBtn = document.getElementById("mode-btn");
const modeLabel = document.getElementById("mode-label");
const btnClear = document.getElementById("btn-clear");
const btnHide = document.getElementById("btn-hide");
const btnSettings = document.getElementById("btn-settings");
const btnAxWarn = document.getElementById("btn-ax-warn");
const toast = document.getElementById("hud-toast");

let toastTimer = null;
function showToast(msg) {
  if (!toast) return;
  toast.textContent = msg;
  toast.classList.add("visible");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toast.classList.remove("visible");
  }, 1900);
}

// Subtle Audio Feedback
let audioCtx = null;
function playAudioCue(type) {
  if (!settingsState.play_sounds) return;
  try {
    if (!audioCtx) audioCtx = new (window.AudioContext || window.webkitAudioContext)();
    const osc = audioCtx.createOscillator();
    const gain = audioCtx.createGain();
    osc.connect(gain);
    gain.connect(audioCtx.destination);

    if (type === "pop") {
      osc.frequency.setValueAtTime(600, audioCtx.currentTime);
      osc.frequency.exponentialRampToValueAtTime(320, audioCtx.currentTime + 0.04);
    } else {
      osc.frequency.setValueAtTime(880, audioCtx.currentTime);
      osc.frequency.exponentialRampToValueAtTime(1240, audioCtx.currentTime + 0.035);
    }
    gain.gain.setValueAtTime(0.08, audioCtx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.04);
    osc.start();
    osc.stop(audioCtx.currentTime + 0.045);
  } catch (e) {}
}

// Apply Visual Theme & Accent
function applyTheme(theme) {
  if (theme === "dark") {
    document.body.classList.add("theme-dark");
  } else if (theme === "light") {
    document.body.classList.remove("theme-dark");
  } else {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.body.classList.toggle("theme-dark", prefersDark);
  }
}

function applyAccent(colorName) {
  const hex = ACCENT_COLORS[colorName] || ACCENT_COLORS.orange;
  document.documentElement.style.setProperty("--accent", hex);
  document.documentElement.style.setProperty(
    "--accent-hover",
    colorName === "orange" ? "#ff8533" : hex
  );
  document.documentElement.style.setProperty(
    "--accent-light",
    `rgba(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)}, 0.12)`
  );
}

// 1. Smooth Window Dragging Anywhere on Header / Pill
function setupDragging() {
  const triggerDrag = (e) => {
    if (
      e.target.closest("button") ||
      e.target.closest(".slot-item") ||
      e.target.closest("#slots-container")
    ) {
      return;
    }
    invoke("start_dragging").catch(() => {});
  };

  hudPill.addEventListener("mousedown", triggerDrag);
  dragHandle.addEventListener("mousedown", () => {
    invoke("start_dragging").catch(() => {});
  });
}

// 2. Generate the Dynamic Slot Elements
function initSlots(count = 9) {
  slotCount = count;
  slotsContainer.innerHTML = "";
  for (let i = 0; i < slotCount; i++) {
    const el = document.createElement("div");
    el.className = "slot-item";
    el.id = `slot-${i}`;
    el.title = `Slot ${i + 1}`;

    el.innerHTML = `
      <span class="slot-num">${i + 1}</span>
      <div class="slot-icon" id="icon-${i}">
        <span class="slot-empty-dot"></span>
      </div>
    `;

    el.addEventListener("click", () => handleSlotClick(i));
    slotsContainer.appendChild(el);
  }
}

// 3. Render Slots
function renderSlots(newSlots, count = 0, newSlotCount = null) {
  if (newSlotCount && newSlotCount !== slotCount) {
    initSlots(newSlotCount);
  }

  slots = newSlots;
  autoCount = count;

  for (let i = 0; i < slotCount; i++) {
    const item = slots[i];
    const el = document.getElementById(`slot-${i}`);
    const icon = document.getElementById(`icon-${i}`);
    if (!el || !icon) continue;

    if (!item) {
      el.className = "slot-item";
      icon.innerHTML = `<span class="slot-empty-dot"></span>`;
      el.title = `Slot ${i + 1} (Empty)`;
      continue;
    }

    el.className = "slot-item has-content";
    if (selectedSlotIndex === i) {
      el.classList.add("is-selected");
    }

    if (item.kind === "text") {
      const preview = item.text.length > 25 ? item.text.substring(0, 25) + "..." : item.text;
      el.title = `Slot ${i + 1}: "${preview}"`;
      icon.innerHTML = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line></svg>`;
    } else if (item.kind === "image") {
      el.title = `Slot ${i + 1}: [Image]`;
      icon.innerHTML = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect><circle cx="8.5" cy="8.5" r="1.5"></circle><polyline points="21 15 16 10 5 21"></polyline></svg>`;
    } else if (item.kind === "files") {
      const fName = item.files[0] ? item.files[0].split("/").pop() : "File";
      el.title = `Slot ${i + 1}: 📁 ${fName}`;
      icon.innerHTML = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>`;
    }
  }

  updateModeDisplay();
}

function updateModeDisplay(infoText = "") {
  modeBtn.className = `mode-badge mode-${currentMode}`;

  if (currentMode === "copy") {
    modeLabel.textContent = "COPY";
    showToast(infoText || `COPY: Select text & tap 1-${slotCount} to save`);
  } else if (currentMode === "cut") {
    modeLabel.textContent = "CUT";
    showToast(infoText || `CUT: Select item & tap 1-${slotCount} (copy fallback)`);
  } else if (currentMode === "paste") {
    modeLabel.textContent = "PASTE";
    showToast(infoText || `PASTE: Tap 1-${slotCount} to paste (Esc to exit)`);
  } else {
    modeLabel.textContent = autoCount > 0 ? `AUTO (${autoCount})` : "AUTO";
    if (infoText) showToast(infoText);
  }
}

// 4. Handle Slot Click
async function handleSlotClick(index) {
  if (currentMode === "copy" || currentMode === "cut") {
    await invoke("assign_to_slot", { slotIndex: index });
    playAudioCue("save");
    showToast(`Saved to Slot ${index + 1}!`);
  } else {
    selectedSlotIndex = index;
    renderSlots(slots, autoCount);

    const el = document.getElementById(`slot-${index}`);
    if (el) {
      el.classList.add("active-pulse");
      setTimeout(() => el.classList.remove("active-pulse"), 400);
    }

    await invoke("select_slot", { slotIndex: index });
    playAudioCue("paste");

    if (currentMode === "paste") {
      showToast(`Pasted Slot ${index + 1}`);
    } else {
      showToast(`Slot ${index + 1} ready on clipboard! (Cmd+V to paste)`);
    }
  }
}

// 5. Interactive Mode Button Click: Cycle Auto ➔ Copy ➔ Cut ➔ Paste ➔ Auto
modeBtn.addEventListener("click", async () => {
  let nextMode = "idle";
  if (currentMode === "idle") {
    nextMode = "copy";
  } else if (currentMode === "copy") {
    nextMode = "cut";
  } else if (currentMode === "cut") {
    nextMode = "paste";
  } else {
    nextMode = "idle";
  }

  currentMode = nextMode;
  await invoke("set_app_mode", { newMode: nextMode });
  updateModeDisplay();
});

btnClear.addEventListener("click", async () => {
  selectedSlotIndex = null;
  await invoke("clear_all_slots");
  playAudioCue("pop");
  showToast("All slots cleared");
});

btnHide.addEventListener("click", async () => {
  await invoke("dismiss_hud");
});

if (btnSettings) {
  btnSettings.addEventListener("click", async () => {
    await invoke("open_settings_window");
  });
}

// Accessibility prompt click
btnAxWarn.addEventListener("click", async () => {
  await invoke("prompt_accessibility");
  showToast("Opening Accessibility settings...");
});

// Keyboard navigation when HUD has focus
window.addEventListener("keydown", async (e) => {
  if (e.key >= "1" && e.key <= "9") {
    e.preventDefault();
    const idx = parseInt(e.key, 10) - 1;
    if (idx < slotCount) {
      await handleSlotClick(idx);
    }
  } else if (e.key === "Escape") {
    e.preventDefault();
    await invoke("dismiss_hud");
  } else if (e.metaKey && e.key === ",") {
    e.preventDefault();
    await invoke("open_settings_window");
  }
});

// Setup Tauri IPC Events
async function setupEvents() {
  try {
    await listen("slots-changed", (event) => {
      renderSlots(
        event.payload.slots,
        event.payload.auto_count || 0,
        event.payload.slot_count || slotCount
      );
    });

    await listen("slot-selected", (event) => {
      selectedSlotIndex = event.payload.slotIndex;
      renderSlots(slots, autoCount);
    });

    await listen("mode-changed", (event) => {
      currentMode = event.payload.mode;
      updateModeDisplay(event.payload.info);
    });

    await listen("accessibility-status", (event) => {
      const isTrusted = event.payload;
      if (isTrusted) {
        btnAxWarn.classList.add("hidden");
      } else {
        btnAxWarn.classList.remove("hidden");
      }
    });

    await listen("settings-changed", (event) => {
      const s = event.payload;
      if (s) {
        settingsState = s;
        applyTheme(s.theme);
        applyAccent(s.accent_color);
        if (s.slot_count !== slotCount) {
          initSlots(s.slot_count);
        }
      }
    });

    // Initial Accessibility Check
    const trusted = await invoke("check_accessibility");
    if (!trusted) {
      btnAxWarn.classList.remove("hidden");
    } else {
      btnAxWarn.classList.add("hidden");
    }

    // Initial Settings
    try {
      const initialSettings = await invoke("get_settings");
      if (initialSettings) {
        settingsState = initialSettings;
        applyTheme(initialSettings.theme);
        applyAccent(initialSettings.accent_color);
        slotCount = initialSettings.slot_count || 9;
      }
    } catch (e) {
      console.warn("Could not load initial settings:", e);
    }

    initSlots(slotCount);

    // Initial Slots state
    const initial = await invoke("get_slots");
    if (initial && initial.slots) {
      if (initial.is_paste) currentMode = "paste";
      else if (initial.is_cut) currentMode = "cut";
      else if (initial.is_copy) currentMode = "copy";
      else currentMode = "idle";
      renderSlots(initial.slots, initial.auto_count || 0, initial.slot_count);
    }
  } catch (err) {
    console.error("IPC listener error:", err);
  }
}

initSlots(slotCount);
setupDragging();
setupEvents();
