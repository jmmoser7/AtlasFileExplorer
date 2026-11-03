const assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
class Surface{
  constructor(){this.handlers={};this.hidden=false;this.clientWidth=800;this.clientHeight=700;this.classList={add(){},remove(){}};this.capture=false;}
  addEventListener(name,fn){(this.handlers[name]??=[]).push(fn);}
  fire(type,props={}){const e={type,button:0,pointerId:1,clientX:100,clientY:100,deltaY:80,target:this,preventDefault(){this.prevented=true;},stopImmediatePropagation(){},...props};for(const fn of this.handlers[type]||[])fn(e);return e;}
  closest(){return null;}matches(){return false;}setPointerCapture(){this.capture=true;}hasPointerCapture(){return this.capture;}releasePointerCapture(){this.capture=false;}getBoundingClientRect(){return{left:0,top:0};}
}
const win=new Surface(),doc={activeElement:null,querySelector(){return null;}};
vm.runInNewContext(fs.readFileSync(__dirname+'/canvas-navigation.js','utf8'),{window:win,document:doc,Element:Surface});
const canvas=new Surface(),camera={x:0,y:0,zoom:1};let contexts=0;
const nav=win.StudioCanvasNavigation.bind({canvas,camera,transform(){},closeMenus(){},context(){contexts++;}});
canvas.fire('pointerdown',{button:2});canvas.fire('pointermove',{clientX:150,clientY:130});canvas.fire('pointerup',{button:2,clientX:150,clientY:130});canvas.fire('contextmenu');
assert.deepEqual(camera,{x:50,y:30,zoom:1});assert.equal(contexts,0,'right drag must not open a menu');
canvas.fire('pointerdown',{button:2});canvas.fire('pointerup',{button:2});canvas.fire('contextmenu');assert.equal(contexts,1,'stationary right click opens one menu');
canvas.fire('pointerdown',{button:1});canvas.fire('pointermove',{clientX:120});canvas.fire('pointerup',{button:1});assert.equal(camera.x,70);
win.fire('keydown',{code:'Space'});canvas.fire('pointerdown');canvas.fire('pointermove',{clientY:125});canvas.fire('pointerup');win.fire('keyup',{code:'Space'});assert.equal(camera.y,55);
const text=new Surface();text.matches=()=>true;canvas.fire('wheel',{target:text});assert.ok(camera.zoom<1,'unfocused editor hover must zoom');
nav.zoom(1);doc.activeElement=text;canvas.fire('wheel',{target:text});assert.equal(camera.zoom,1,'focused editor owns ordinary wheel');
canvas.fire('wheel',{target:text,ctrlKey:true});assert.ok(camera.zoom<1,'explicit zoom remains available');
nav.zoom(999);assert.equal(camera.zoom,128);nav.zoom(0);assert.equal(camera.zoom,1/128);
console.log('Slate navigation gestures, popup suppression and focused-wheel ownership: passed');
