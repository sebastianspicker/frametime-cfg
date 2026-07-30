const nav=[...document.querySelectorAll("[data-panel]")];
const panels=[...document.querySelectorAll("[data-panel-view]")];
const toast=document.querySelector("#toast");
let toastTimer;

function showPanel(name,focus=true){
  panels.forEach(panel=>{panel.hidden=panel.dataset.panelView!==name});
  nav.forEach(button=>{const active=button.dataset.panel===name;button.classList.toggle("is-active",active);active?button.setAttribute("aria-current","page"):button.removeAttribute("aria-current")});
  if(focus){document.querySelector(`[data-panel-view="${name}"] h1`)?.focus({preventScroll:true});window.scrollTo({top:0,behavior:"smooth"})}
}

function showToast(message){clearTimeout(toastTimer);toast.textContent=message;toast.classList.add("is-visible");toastTimer=setTimeout(()=>toast.classList.remove("is-visible"),3800)}

nav.forEach(button=>button.addEventListener("click",()=>showPanel(button.dataset.panel,false)));
document.querySelectorAll("[data-go]").forEach(button=>button.addEventListener("click",()=>showPanel(button.dataset.go)));
document.querySelectorAll("[data-sim-action]").forEach(button=>button.addEventListener("click",()=>{
  const action=button.textContent.replace("Simulated","").trim();
  showToast(`${action}: simulated response only. No command ran and no data was written.`);
  if(button.dataset.simAction==="scan"){const time=document.querySelector("#scan-time");if(time)time.textContent="Fixture scan complete: 5 rows loaded from sanitized demo data."}
}));
document.querySelectorAll('input[name="profile"]').forEach(input=>input.addEventListener("change",()=>{document.querySelector(".sidebar-state span:first-child").textContent=`Profile: ${input.value}`;showToast(`${input.value} profile selected for this browser view. No suite state was written.`)}));
document.querySelector("#refresh-rate")?.addEventListener("change",event=>{const values={"144 Hz":"131 FPS","240 Hz":"218 FPS","360 Hz":"327 FPS"};document.querySelector("#cap-value").textContent=values[event.target.value]});
document.addEventListener("keydown",event=>{if(!event.ctrlKey||event.altKey||event.metaKey)return;const index=Number(event.key)-1;const button=nav.at(index);if(index>=0&&button){event.preventDefault();showPanel(button.dataset.panel)}});
