const invoke = window.__TAURI__?.core?.invoke;
const views = {
  menu: document.querySelector("#menu-view"),
  campaign: document.querySelector("#campaign-view"),
  online: document.querySelector("#online-view"),
};

const map = document.querySelector("#campaign-map");
const summary = document.querySelector("#turn-summary");
const detail = document.querySelector("#province-detail");
const chronicle = document.querySelector("#chronicle");
const errorBox = document.querySelector("#error");
const endTurnButton = document.querySelector("#end-turn");
const newCampaignButton = document.querySelector("#new-campaign");
const campaignButton = document.querySelector("#open-campaign");
const onlineButton = document.querySelector("#open-online");

let campaign;
let selectedProvinceId;

function showView(name) {
  for (const [viewName, element] of Object.entries(views)) {
    element.hidden = viewName !== name;
  }
  errorBox.hidden = true;
}

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

function renderCampaign() {
  renderSummary();
  renderMap();
  renderProvinceDetail();
  renderChronicle();
}

function reportError(error) {
  errorBox.hidden = false;
  errorBox.textContent = String(error);
}

async function ensureCampaignLoaded() {
  if (campaign) return;
  if (!invoke) {
    throw new Error("Campaign mode uses the Rust game core through Tauri. Run the app with `cargo tauri dev`.");
  }

  campaign = await invoke("campaign_state");
  selectedProvinceId = campaign.provinces[0]?.id;
  renderCampaign();
}

async function runCommand(command) {
  endTurnButton.disabled = true;
  newCampaignButton.disabled = true;
  try {
    campaign = await invoke(command);
    if (!campaign.provinces.some((province) => province.id === selectedProvinceId)) {
      selectedProvinceId = campaign.provinces[0]?.id;
    }
    renderCampaign();
  } catch (error) {
    reportError(error);
  } finally {
    endTurnButton.disabled = false;
    newCampaignButton.disabled = false;
  }
}

campaignButton.addEventListener("click", async () => {
  showView("campaign");
  try {
    await ensureCampaignLoaded();
  } catch (error) {
    reportError(error);
  }
});

onlineButton.addEventListener("click", () => showView("online"));
for (const button of document.querySelectorAll("[data-back-to-menu]")) {
  button.addEventListener("click", () => showView("menu"));
}

endTurnButton.addEventListener("click", () => runCommand("end_turn"));
newCampaignButton.addEventListener("click", () => runCommand("start_new_campaign"));

showView("menu");
