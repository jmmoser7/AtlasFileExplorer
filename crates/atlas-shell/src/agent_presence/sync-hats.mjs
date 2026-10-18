// Regenerate the native interpreter's hat asset from the approved vector art
// and the user's exact neutral calibration. No geometry is re-authored here.
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
const root = process.cwd();
const design = path.join(root, 'design/agent-presence-2026-10-17');
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(path.join(design,'hats.js'),'utf8'),context);
const source = context.PresenceHats;
const fits = JSON.parse(fs.readFileSync(path.join(design,'hat-neutral-fits.json'),'utf8'));
const hats = {};
for (const item of source.items) {
  hats[item.id] = {
    label:item.name,
    svg:item.id==='none'?'':source.svg(item.id)
      .replaceAll('currentColor','#858585')
      .replaceAll('var(--hat-seam,#fff)','#ffffff'),
    ...(fits.hats?.[item.id] || fits.fits?.[item.id] ? {fit:fits.hats?.[item.id] || fits.fits?.[item.id]} : {})
  };
}
if (Object.values(hats).filter(h=>h.fit).length!==26) throw new Error('All26 user hat fits are required.');
const asset = path.join(root,'crates/atlas-shell/assets/agent-hats.json');
fs.writeFileSync(asset,JSON.stringify({revision:source.revision,basis:fits.basis,motion:{horizontalOffset:4,verticalOffset:3,yawCompression:.045,pitchCompression:.025},order:source.items.map(x=>x.id),hats},null,2)+'\n');
const masterPath=path.join(root,'crates/atlas-shell/assets/agent-presence.json');
const master=JSON.parse(fs.readFileSync(masterPath,'utf8'));
delete master.hats;
fs.writeFileSync(masterPath,JSON.stringify(master,null,2)+'\n');
console.log(`Synchronized ${source.items.length} hats and26 exact user fits.`);
