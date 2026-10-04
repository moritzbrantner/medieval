# Campaign battle acceptance

`npm run test:e2e:campaign` exercises the released campaign WASM and tactical
WASM/WebGPU adapters in Chromium. The Pages build runs it after staging the
actual release artifacts in `web/pkg`; it also runs the tactical controls suite.
For local acceptance, stage those same artifacts, run `npm ci`, install Chromium
with `npx playwright install chromium`, then run the campaign command.

The vertical slice starts a campaign and loads a validated starting variant:
England keeps its normal army, France begins with 60 levy, 10 spearmen and
10 archers in Flanders, and Paris uses the existing forest field profile. This
keeps France's first strategic move away from Normandy while England recruits,
and tests field combat independently of future siege development. The fixture
changes starting campaign data only and passes through the production Rust save
loader. It never changes tactical state, ticks, casualties or battle outcomes.

The player recruits 20 archers through the campaign UI, ends the turn with the
real deterministic opponent, attacks Paris and chooses Fight. The tactical
renderer must project all 60 campaign archers and the retained army provenance.
Physical clicks select the army, a semantic control changes formation, and
physical right-clicks issue engagement orders, retargeting remaining enemies as
needed. The simulation advances on its normal frame clock until core victory.

Acceptance then verifies source-aware casualty conservation, the surviving
campaign roster, capture of Paris, the battle report, and identical state after
Save, reload and Load. Read-only tactical status and viewport projection exports
provide observations and pointer coordinates; they do not bypass commands.
Screenshots record issued orders and the reloaded captured province/report.

The same command covers played withdrawal and auto-resolve, cancellation that
preserves pending state, pre-battle and completed-outcome reloads, failed browser
storage rollback and rejected corrupt pending provenance. These tests retain the
existing browser-storage lookup key so historical schema-1 saves remain reachable;
Rust owns save version migration and validation.
