const saveCampaignButton = document.querySelector("#save-campaign");
const loadCampaignButton = document.querySelector("#load-campaign");
const loadAutosaveButton = document.querySelector("#load-autosave");
const saveStatus = document.querySelector("#save-status");

function setSaveControlsDisabled(disabled) {
  saveCampaignButton.disabled = disabled;
  loadCampaignButton.disabled = disabled;
  loadAutosaveButton.disabled = disabled;
}

async function saveCampaignManually() {
  if (!invoke || !campaign) return;

  errorBox.hidden = true;
  setSaveControlsDisabled(true);
  try {
    await invoke("save_campaign");
    saveStatus.textContent = `Manual save written for turn ${campaign.turn}.`;
  } catch (error) {
    reportError(error);
  } finally {
    setSaveControlsDisabled(false);
  }
}

async function loadCampaignSave(command, label) {
  if (!invoke) return;

  errorBox.hidden = true;
  setSaveControlsDisabled(true);
  endTurnButton.disabled = true;
  newCampaignButton.disabled = true;
  try {
    campaign = await invoke(command);
    [playerFaction, campaignWinner] = await Promise.all([
      invoke("campaign_player_faction"),
      invoke("campaign_winner"),
    ]);
    playerFactionSelect.value = playerFaction;
    clearMovementSelection();
    selectedProvinceId = campaign.provinces.find((province) => province.owner === playerFaction)?.id
      ?? campaign.provinces[0]?.id;
    await refreshRecruitmentOptions();
    renderCampaign();
    saveStatus.textContent = `Loaded ${label} save from turn ${campaign.turn}.`;
  } catch (error) {
    reportError(error);
  } finally {
    setSaveControlsDisabled(false);
    syncCampaignActions();
  }
}

saveCampaignButton.addEventListener("click", saveCampaignManually);
loadCampaignButton.addEventListener("click", () => loadCampaignSave("load_manual_campaign", "manual"));
loadAutosaveButton.addEventListener("click", () => loadCampaignSave("load_autosave_campaign", "autosave"));

if (!invoke) {
  setSaveControlsDisabled(true);
  saveStatus.textContent = "Native save files are available in the Tauri game build.";
}
