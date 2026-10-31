/* Browser translation of native image layout constants. World units, not zoomed pixels.

   References: agent_image_layout.rs, blister.rs, generator_history.rs, wire_host.rs. */

(function (root) {

  'use strict';

  function layout(width, height, connected, expanded, historyCount = 1, modelLabelWidth = 96) {

    const count = connected ? 8 : 7, natural = count * 48 + 168;

    const attached = natural <= width - 16 && natural * 40 <= width * height * .2;

    const panelWidth = attached ? width : 420;

    const panelX = (width - panelWidth) / 2;

    const panelY = attached ? height : height + 14 + 30 + 8;

    const insetX=8,top=2,modelWidth=Math.max(56,Math.min(120,modelLabelWidth));

    const controls={model:[insetX,top,modelWidth,24]};

    controls.more=[insetX+modelWidth+1,top,14,24];

    controls.play=[panelWidth-insetX-20,top+2,20,20];

    controls.count=[controls.play[0]-4-32,top,32,24];

    const hx=controls.more[0]+26;

    controls.history=[hx,top-1,Math.max(0,controls.count[0]-4-hx),26];

    controls.prompt=[insetX,28,panelWidth-insetX*2,32];

    const bandWidth = attached ? (expanded ? Math.min(width,count*48+96) : natural) : count*30+(count-1)*7;

    const bandY = attached ? height-(expanded?40:20) : height+14;

    const inset = attached ? Math.min(expanded?42:84,bandWidth*.24) : 0;

    const pitch = attached ? (bandWidth-2*inset)/count : 37;

    const size = attached ? Math.min(31.2,pitch*.82) : 30;

    const buttons = Array.from({length:count},(_,i)=>[(width-bandWidth)/2+inset+pitch*(attached?i+.5:i)+(attached?-size/2:0),bandY+(attached?(40-size)/2:0),size,size]);

    return {attached, panelWidth,panelX,panelY,controls,buttons,bandWidth,bandY};

  }

  // Circle area, rather than its bounding box: 65% outside, 35% inside.

  const radius=6,offset=1.427;

  function ports(width){return [[-offset,48],[-offset,90],[-offset,132],[width+offset,48]];}

  // Original 4-unit clearance reduced by 65% to 1.4. Cubic shoulders

  // meet the straight image edge tangentially; the inner wall is circular.

  function notch(width,y,right=false){

    const r=radius+1.4,f=(radius+1.4)*.4,x=2,dy=Math.sqrt(r*r-(x+offset)**2);

    const X=v=>right?width-v:v,side=right?0:1;

    return `M${X(0)},${y-r-f} C${X(0)},${y-r} ${X(-.2)},${y-dy-1.15} ${X(x)},${y-dy} A${r},${r} 0 0 ${side} ${X(x)},${y+dy} C${X(-.2)},${y+dy+1.15} ${X(0)},${y+r} ${X(0)},${y+r+f} L${X(-16)},${y+r+f} L${X(-16)},${y-r-f} Z`;

  }

  function resultGrid(width,count){
    const size=24,gap=4,columns=Math.max(1,Math.min(8,Math.floor((width+gap)/(size+gap))));
    const rows=Math.ceil(count/columns),positions=Array.from({length:count},(_,i)=>({x:(i%columns)*(size+gap)+(Math.floor(i/columns)%2)*size/2,y:Math.floor(i/columns)*(size+gap)}));
    return {columns,rows,width:Math.max(0,...positions.map(p=>p.x+size)),height:rows?rows*size+(rows-1)*gap:0,positions};
  }
  function bezier(a,b) {const d=Math.max(Math.abs(b[0]-a[0])*.5,Math.abs(b[1]-a[1])*.75);return `M${a[0]},${a[1]} C${a[0]+d},${a[1]} ${b[0]-d},${b[1]} ${b[0]},${b[1]}`;}

  const api={layout,bezier,ports,notch,resultGrid,portRadius:radius,portOffset:offset}; root.ImageStudioLayout=api;

  if(typeof module!=='undefined')module.exports=api;

})(typeof window==='undefined'?globalThis:window);
