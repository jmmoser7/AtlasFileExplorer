"""Rasterize exported native InkMesh triangles for comparison with the SVG source.

This deliberately does not render SVG for the native columns. Uses premultiplied
white ink with barycentrically interpolated vertex alpha, like the egui mesh.
"""
from pathlib import Path
import base64, io, json
import numpy as np
from PIL import Image

ROOT = Path(__file__).parent
OUT = ROOT / 'native-audit'
DEFS = json.loads((ROOT.parents[1] / 'crates/atlas-shell/assets/tool-icons.json').read_text())

def raster(mesh, size):
    ss = 4
    n = size * ss
    alpha = np.zeros((n, n), dtype=np.float64)
    verts = np.array(mesh['vertices'], dtype=float)
    verts[:, :2] *= n / 24
    for a, b, c in np.array(mesh['indices']).reshape(-1, 3):
        tri = verts[[a,b,c]]
        lo = np.maximum(0, np.floor(tri[:, :2].min(axis=0)).astype(int))
        hi = np.minimum(n, np.ceil(tri[:, :2].max(axis=0)).astype(int))
        if np.any(hi <= lo): continue
        x,y = np.meshgrid(np.arange(lo[0],hi[0])+.5, np.arange(lo[1],hi[1])+.5)
        x0,y0,aa = tri[0]; x1,y1,ab = tri[1]; x2,y2,ac = tri[2]
        den = (y1-y2)*(x0-x2)+(x2-x1)*(y0-y2)
        if abs(den)<1e-10: continue
        wa=((y1-y2)*(x-x2)+(x2-x1)*(y-y2))/den
        wb=((y2-y0)*(x-x2)+(x0-x2)*(y-y2))/den
        wc=1-wa-wb
        inside=(wa>=-1e-9)&(wb>=-1e-9)&(wc>=-1e-9)
        ink=np.where(inside,np.clip(wa*aa+wb*ab+wc*ac,0,1),0)
        dst=alpha[lo[1]:hi[1],lo[0]:hi[0]]
        dst[:]=ink+dst*(1-ink)
    alpha=alpha.reshape(size,ss,size,ss).mean(axis=(1,3))
    rgba=np.full((size,size,4),255,dtype=np.uint8)
    rgba[:,:,3]=np.rint(alpha*255).astype(np.uint8)
    return Image.fromarray(rgba)

def embed(im):
    b=io.BytesIO();im.save(b,format='PNG')
    return 'data:image/png;base64,'+base64.b64encode(b.getvalue()).decode()

before=json.loads((OUT/'before.json').read_text())
after=json.loads((OUT/'after.json').read_text()) if (OUT/'after.json').exists() else None
names=['Frame','Portals','Shapes','Text','Actions','ObjectProperties','DocumentSettings','Selection']
names += [n for n in DEFS if n not in names]
rows=[]
for name in names:
    d=DEFS[name]
    svg=f'<svg width="72" height="72" viewBox="0 0 24 24" fill="{"white" if d["filled"] else "none"}" stroke="white" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="{d["path"]}"/></svg>'
    cols=[svg, f'<img src="{embed(raster(before[name],72))}" width="72" height="72">']
    if after:
        cols += [f'<img src="{embed(raster(after[name],72))}" width="72" height="72">', ''.join(f'<img src="{embed(raster(after[name],sz))}" width="{sz}" height="{sz}">' for sz in [16,20,24,32])]
    rows.append(f'<div class="row"><span>{name}</span>'+''.join(f'<div>{col}</div>' for col in cols)+'</div>')
html='''<!doctype html><html><meta charset="utf-8"><title>Native icon rendering audit</title><style>
body{margin:0;padding:28px;background:#101317;color:#eee;font:14px/1.5 Segoe UI,sans-serif}h1{font-size:24px}p{color:#aeb7c4;max-width:900px}.row{display:grid;grid-template-columns:minmax(85px,1fr) repeat(3,76px) 144px;align-items:center;gap:12px;padding:12px 0;border-bottom:1px solid #30363d}.row div{display:flex;gap:12px;align-items:center}.head{position:sticky;top:0;background:#101317;font-weight:600}img{object-fit:contain}
</style><h1>Native mesh verification</h1><p>SVG reference versus actual native triangle geometry. The before column reproduces the missing edges. Corrected native columns are rasterized from the exact InkMesh vertices and indices used by the application. Final column: 16, 20, 24, 32 px.</p><div class="row head"><span>Icon</span><span>SVG reference</span><span>Native before</span><span>Native corrected</span><span>Actual sizes</span></div>'''+''.join(rows)+'</html>'
(OUT/'index.html').write_text(html,encoding='utf-8')
if after:
    strip=Image.new('RGBA',(8*64,72),(16,19,23,255))
    for i,name in enumerate(names[:8]):strip.alpha_composite(raster(after[name],32),(i*64+16,20))
    strip.save(OUT/'corrected-primary-strip.png')
print(OUT/'index.html')
