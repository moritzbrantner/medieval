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
const pendingBattle = document.querySelector("#pending-battle");
const pendingBattleDetail = document.querySelector("#pending-battle-detail");
const errorBox = document.querySelector("#error");
const endTurnButton = document.querySelector("#end-turn");
const newCampaignButton = document.querySelector("#new-campaign");
const campaignButton = document.querySelector("#open-campaign");
const onlineButton = document.querySelector("#open-online");

let campaign;
let selectedProvinceId;
let selectedArmyId;
let legalDestinationIds = [];

function showView(name) {
  for (const [viewName, element] of Object.entries(views)) {
    element.hidden = viewName !== name;
  }
  errorBox.hidden = true;
}

function factionName(id) {
  return campaign.factions.find((faction) => faction.id === id)?.name ?? id;
}

function provinceName(id) {
  return campaign.provinces.find((province) => province.id === id)?.name ?? id;
}

function armyInProvince(provinceId) {
  return campaign.armies.filter((army) => army.province === provinceId);
}

function renderSummary() {
  const faction = factionName(campaign.activeFaction);
  const battleSuffix = campaign.pendingBattle ? " · Battle pending" : "";
  summary.textContent = `${campaign.year} · Turn ${campaign.turn} · ${faction}${battleSuffix}`;
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
  const neighborNames = province.neighbors.map(provinceName).join(", ");
  neighbors.textContent = `Borders: ${neighborNames || "none"}.`;

  detail.append(title, owner, neighbors);

  for (const army of armyInProvince(province.id)) {
    const armyCard = document.createElement("div");
    armyCard.className = "army";

    const armyText = document.createElement("p");
    armyText.textContent = `${factionName(army.owner)} army: ${army.levy} levy · ${army.spearmen} spearmen · ${army.archers} archers · ${army.knights} knights`;

    const movementStatus = document.createElement("p");
    movementStatus.className = "army-status";
    movementStatus.textContent = army.movedThisTurn ? "Movement spent this turn." : "Movement available if Rust permits it.";

    const selectArmyButton = document.createElement("button");
    selectArmyButton.type = "button";
    selectArmyButton.className = "army-select";
    selectArmyButton.textContent = army.id === selectedArmyId ? "Army selected" : "Issue movement order";
    selectArmyButton.addEventListener("click", () => selectArmy(army.id));

    armyCard.append(armyText, movementStatus, selectArmyButton);
    detail.append(armyCard);
  }

  if (selectedArmyId) {
    const orderHint = document.createElement("p");
    orderHint.className = "order-hint";
    orderHint.textContent = legalDestinationIds.length
      ? "Rust has marked the highlighted provinces as legal destinations."
      : "Rust returned no legal destinations for the selected army.";
    detail.append(orderHint);
  }
}

function renderMap() {
  map.replaceChildren();

  for (const province of campaign.provinces) {
    const isLegalDestination = legalDestinationIds.includes(province.id);
    const button = document.createElement("button");
    button.type = "button";
    button.className = `province owner-${province.owner}${isLegalDestination ? " legal-destination" : ""}`;
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
    if (isLegalDestination) {
      const legal = document.createElement("small");
      legal.className = "legal-label";
      legal.textContent = "Legal move";
      button.append(legal);
    }

    button.addEventListener("click", () => {
      if (selectedArmyId && isLegalDestination) {
        moveSelectedArmy(province.id);
        return;
      }

      selectedProvinceId = province.id;
      clearMovementSelection();
      renderMap();
      renderProvinceDetail();
    });
    map.append(button);
  }
}

function renderPendingBattle() {
  const battle = campaign.pendingBattle;
  pendingBattle.hidden = !battle;
  pendingBattleDetail.replaceChildren();

  if (!battle) return;

  const text = document.createElement("p");
  text.textContent = `${factionName(battle.attackerFaction)} has marched from ${provinceName(battle.fromProvince)} into ${provinceName(battle.targetProvince)}, held by ${factionName(battle.defenderFaction)}.`;

  const note = document.createElement("p");
  note.className = "battle-note";
  note.textContent = "Combat is intentionally unresolved. The later battle slice will consume this Rust-owned pending battle.";

  pendingBattleDetail.append(text, note);
}

function renderChronicle() {
  chronicle.replaceChildren();
  for (const entry of [...campaign.log].reverse().slice(0, 6)) {
    const item = document.createElement("li");
    item.textContent = entry;
    chronicle.append(item);
  }
}

function syncCampaignActions() {
  endTurnButton.disabled = Boolean(campaign?.pendingBattle);
  newCampaignButton.disabled = false;
}

function renderCampaign() {
  renderSummary();
  renderMap();
  renderProvinceDetail();
  renderPendingBattle();
  renderChronicle();
  syncCampaignActions();
}

function reportError(error) {
  errorBox.hidden = false;
  errorBox.textContent = String(error);
}

function clearMovementSelection() {
  selectedArmyId = undefined;
  legalDestinationIds = [];
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

async function selectArmy(armyId) {
  if (!invoke) return;

  errorBox.hidden = true;
  try {
    legalDestinationIds = await invoke("legal_army_destinations", { armyId });
    selectedArmyId = armyId;
    const army = campaign.armies.find((item) => item.id === armyId);
    selectedProvinceId = army?.province ?? selectedProvinceId;
    renderCampaign();
  } catch (error) {
    clearMovementSelection();
    renderCampaign();
    reportError(error);
  }
}

async function moveSelectedArmy(destination) {
  if (!invoke || !selectedArmyId) return;

  const armyId = selectedArmyId;
  endTurnButton.disabled = true;
  newCampaignButton.disabled = true;
  errorBox.hidden = true;
  try {
    campaign = await invoke("move_army", { armyId, destination });
    selectedProvinceId = destination;
    clearMovementSelection();
    renderCampaign();
  } catch (error) {
    reportError(error);
  } finally {
    syncCampaignActions();
  }
}

async function runCommand(command) {
  endTurnButton.disabled = true;
  newCampaignButton.disabled = true;
  errorBox.hidden = true;
  try {
    campaign = await invoke(command);
    clearMovementSelection();
    if (!campaign.provinces.some((province) => province.id === selectedProvinceId)) {
      selectedProvinceId = campaign.provinces[0]?.id;
    }
    renderCampaign();
  } catch (error) {
    reportError(error);
  } finally {
    syncCampaignActions();
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
