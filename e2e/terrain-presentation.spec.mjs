import { expect, test } from "@playwright/test";

for (const location of ["mountainPass", "forestClearing", "riverFord"]) {
  test(`${location} terrain preserves logical picking and its geometry budget`, async ({ page }, testInfo) => {
    await page.goto(`/battle.html?e2e-controls=1&location=${location}`);
    await page.getByRole("button", { name: "Enter battle" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
    await page.getByRole("button", { name: "Pause", exact: true }).click();
    const status=()=>page.evaluate(()=>window.__medievalControlsE2E.status());
    const before=await status();
    expect(before.battlefieldLocation).toBe(location);
    expect(before.terrainVertices).toBeGreaterThan(0);
    expect(before.terrainVertices).toBeLessThanOrEqual(16_384);
    const canvas=page.locator("#battle-canvas");
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const path=testInfo.outputPath(`${location}.png`);
    await canvas.screenshot({path});
    await testInfo.attach(`${location} battlefield`,{path,contentType:"image/png"});
    const unit=await page.evaluate(()=>window.__medievalControlsE2E.unitViewport("attacker-spears"));
    const box=await canvas.boundingBox();
    await page.mouse.click(box.x+unit.xPx,box.y+unit.yPx);
    const point=await page.evaluate(()=>window.__medievalControlsE2E.groundViewport(22_000,30_000));
    await page.mouse.click(box.x+point.xPx,box.y+point.yPx,{button:"right"});
    const after=await status();
    const destination=after.units.find(unit=>unit.id==="attacker-spears").destination;
    expect(Math.abs(destination.xMm-22_000)).toBeLessThanOrEqual(2);
    expect(Math.abs(destination.yMm-30_000)).toBeLessThanOrEqual(2);
    expect(after.tick).toBe(before.tick);
    expect(after.terrainProfile).toBe(before.terrainProfile);
    expect(after.forestCells).toEqual(before.forestCells);
    expect(after.riverCells).toEqual(before.riverCells);
    expect(after.riverCrossingCells).toEqual(before.riverCrossingCells);
    expect(after.siege).toEqual(before.siege);
    await expect(page.locator("#battle-error")).toBeHidden();
  });
}


test("location changes work when mapped terrain allocations are unavailable", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.evaluate(() => {
    const createBuffer = GPUDevice.prototype.createBuffer;
    GPUDevice.prototype.createBuffer = function(descriptor) {
      if (descriptor.mappedAtCreation && descriptor.label === "Medieval terrain surface and exposed cliffs") {
        throw new RangeError("mapped terrain allocation unavailable");
      }
      return createBuffer.call(this, descriptor);
    };
  });
  for (const location of ["forestClearing", "riverFord", "mountainPass"]) {
    await page.locator("#battle-location").selectOption(location);
    await expect.poll(() => page.evaluate(() => window.__medievalControlsE2E.status().battlefieldLocation)).toBe(location);
    await expect(page.locator("#battle-error")).toBeHidden();
  }
});
