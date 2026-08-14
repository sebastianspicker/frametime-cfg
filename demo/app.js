"use strict";

const navigation = [...document.querySelectorAll("[data-panel]")];
const panels = [...document.querySelectorAll("[data-panel-view]")];
const toast = document.querySelector("#toast");
const toastMessage = document.querySelector("#toast-message");
const profileLabel = document.querySelector("#profile-label");
let toastTimer;

function showPanel(name, moveFocus = true) {
  const target = panels.find((panel) => panel.dataset.panelView === name);
  if (!target) return;

  panels.forEach((panel) => {
    panel.hidden = panel !== target;
  });
  navigation.forEach((button) => {
    const active = button.dataset.panel === name;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
  });

  if (moveFocus) {
    target.querySelector("h1")?.focus({ preventScroll: true });
    window.scrollTo({ top: 0, behavior: "smooth" });
  }
}

function showToast(message) {
  clearTimeout(toastTimer);
  toastMessage.textContent = message;
  toast.hidden = false;
  requestAnimationFrame(() => toast.classList.add("is-visible"));
  toastTimer = setTimeout(() => {
    toast.classList.remove("is-visible");
    setTimeout(() => {
      toast.hidden = true;
    }, 180);
  }, 3800);
}

function activateRelativePanel(offset) {
  const currentIndex = navigation.findIndex((button) => button.classList.contains("is-active"));
  const nextIndex = (currentIndex + offset + navigation.length) % navigation.length;
  navigation[nextIndex].focus();
  showPanel(navigation[nextIndex].dataset.panel, false);
}

navigation.forEach((button) => {
  button.addEventListener("click", () => showPanel(button.dataset.panel, false));
  button.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowRight") {
      event.preventDefault();
      activateRelativePanel(1);
    }
    if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
      event.preventDefault();
      activateRelativePanel(-1);
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const target = event.key === "Home" ? navigation[0] : navigation.at(-1);
      target.focus();
      showPanel(target.dataset.panel, false);
    }
  });
});

document.querySelectorAll("[data-go]").forEach((button) => {
  button.addEventListener("click", () => showPanel(button.dataset.go));
});

document.querySelectorAll("[data-sim-action]").forEach((button) => {
  button.addEventListener("click", () => {
    const action = button.dataset.simAction;
    const messages = {
      scan: "Fixture scan complete: five sanitized rows loaded. No system query ran.",
      setup: "Setup preview prepared for all three phases. No PowerShell command ran.",
      benchmark: "Example benchmark summary recalculated from fixture values only.",
      network: "Example network diagnostics refreshed without contacting a host.",
      video: "Video preview updated in memory. No game file was written.",
      recovery: "Recovery preview opened with sanitized example entries only."
    };
    showToast(messages[action] || "Simulation complete. No command ran and no data was written.");

    if (action === "scan") {
      const scanTime = document.querySelector("#scan-time");
      if (scanTime) scanTime.textContent = "Fixture scan complete · five sanitized rows";
    }
  });
});

document.querySelectorAll('input[name="profile"]').forEach((input) => {
  input.addEventListener("change", () => {
    profileLabel.textContent = `Profile: ${input.value}`;
    showToast(`${input.value} selected for this browser view. No suite state was written.`);
  });
});

document.querySelector("#refresh-rate")?.addEventListener("change", (event) => {
  const values = { "144 Hz": "131 FPS", "240 Hz": "218 FPS", "360 Hz": "327 FPS" };
  document.querySelector("#cap-value").textContent = values[event.target.value];
});

document.querySelector("#toast-close")?.addEventListener("click", () => {
  clearTimeout(toastTimer);
  toast.classList.remove("is-visible");
  toast.hidden = true;
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !toast.hidden) {
    document.querySelector("#toast-close")?.click();
  }
  if (!event.ctrlKey || event.altKey || event.metaKey) return;
  const index = Number(event.key) - 1;
  const button = navigation.at(index);
  if (index >= 0 && button) {
    event.preventDefault();
    button.focus();
    showPanel(button.dataset.panel, false);
  }
});
