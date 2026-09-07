const invoke = window.__TAURI__?.core?.invoke;
const map = document.querySelector("#campaign-map");
const summary = document.querySelector("#turn-summary");
const detail = document.querySelector("#province-detail");
const chronicle = document.querySelector("#chronicle");
const errorBox = document.querySelector("#error");
const endTurnButton = document.querySelector("#end-turn");
const newCampaignButton = document.querySelector("#new-campaign");

let campaign;
let selectedProvinceId;

function factionName(id) {
  return campaign.factions.find((faction) => faction.id === id)?.name ?? id;
}

function armyInProvince(provinceId) {
  return campaign.armies.filter((army) => army.province === provinceId);
}

function renderSummary() {
  const faction = factionName(campaign.activeFaction);
  summary.textContent = `${campaign.year} · Turn ${campaign.turn} · ${faction}`;
}

function renderProvinceDetail() {
  const province = campaign.provinces.find((item) => item.id === selectedProvinceId);
  detail.replaceChildren();

  if (!province) {
    detail.textContent = "Select a province on the map.";
    return;
  }

  const title = document.createElement("h2");
  title.textContent = province.name;

  const owner = document.createElement("p");
  owner.textContent = `Ruled by ${factionName(province.owner)}. Wealth ${province.wealth}.`;

  const neighbors = document.createElement("p");
  const neighborNames = province.neighbors
    .map((id) => campaign.provinces.find((item) => item.id === id)?.name ?? id)
    .join(", ");
  neighbors.textContent = `Borders: ${neighborNames || "none"}.`;

  detail.append(title, owner, neighbors);

  for (const army of armyInProvince(province.id)) {
    const armyText = document.createElement("p");
    armyText.className = "army";
    armyText.textContent = `Army: ${army.levy} levy · ${army.spearmen} spearmen · ${army.archers} archers · ${army.knights} knights`;
    detail.append(armyText);
  }
}

function renderMap() {
  map.replaceChildren();

  for (const province of campaign.provinces) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `province owner-${province.owner}`;
    button.dataset.province = province.id;
    button.setAttribute("aria-pressed", String(province.id === selectedProvinceId));

    const name = document.createElement("strong");
    name.textContent = province.name;

    const owner = document.createElement("span");
    owner.textContent = factionName(province.owner);

    const armies = armyInProvince(province.id);
    const force = document.createElement("small");
    force.textContent = armies.length === 0 ? "No field army" : `${armies.length} field army`;

    button.append(name, owner, force);
    button.addEventListener("click", () => {
      selectedProvinceId = province.id;
      renderMap();
      renderProvinceDetail();
    });
    map.append(button);
  }
}

function renderChronicle() {
  chronicle.replaceChildren();
  for (const entry of [...campaign.log].reverse().slice(0, 6)) {
    const item = document.createElement("li");
    item.textContent = entry;
    chronicle.append(item);
  }
}

function render() {
  renderSummary();
  renderMap();
  renderProvinceDetail();
  renderChronicle();
}

async function runCommand(command) {
  endTurnButton.disabled = true;
  newCampaignButton.disabled = true;
  try {
    campaign = await invoke(command);
    if (!campaign.provinces.some((province) => province.id === selectedProvinceId)) {
      selectedProvinceId = campaign.provinces[0]?.id;
    }
    render();
  } catch (error) {
    errorBox.hidden = false;
    errorBox.textContent = String(error);
  } finally {
    endTurnButton.disabled = false;
    newCampaignButton.disabled = false;
  }
}

async function start() {
  if (!invoke) {
    errorBox.hidden = false;
    errorBox.textContent = "This prototype uses the Rust game core through Tauri. Run it with `cargo tauri dev`.";
    return;
  }

  campaign = await invoke("campaign_state");
  selectedProvinceId = campaign.provinces[0]?.id;
  render();
}

endTurnButton.addEventListener("click", () => runCommand("end_turn"));
newCampaignButton.addEventListener("click", () => runCommand("start_new_campaign"));

start().catch((error) => {
  errorBox.hidden = false;
  errorBox.textContent = String(error);
});
