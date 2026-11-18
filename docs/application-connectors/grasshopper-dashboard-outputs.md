# HTML dashboard outputs for Grasshopper

Place an HTML file on the board as a Web component. Place a Grasshopper definition with an exposed input in a 3D model component. Once the HTML page loads, hover over or select the component to see its named outputs on the right edge. Drag an output grip to the matching input on the Grasshopper component's left edge. Enter the HTML component to interact with its controls. A locked Grasshopper viewport keeps its camera and displays the new result when the solve finishes. Slate saves the wire in the workbook.

Publish the full current state in one script element in the top-level document. Give each output its own UUID and keep that UUID across revisions. The name can change.

```html
<script id="slate-outputs" type="application/slate-outputs+json">
{"version":1,"outputs":[
  {"id":"1b993386-89d8-4b25-b102-452ea7120eb4","name":"Module weights","type":"number","access":"list","value":[0.2,0.8]},
  {"id":"bb388260-d539-49ec-a323-490bca6ed994","name":"Show frame","type":"boolean","access":"item","value":true}
]}
</script>
<script>
  const element = document.getElementById("slate-outputs");
  const state = JSON.parse(element.textContent);
  function setWeights(weights) {
    state.outputs[0].value = weights;
    element.textContent = JSON.stringify(state);
  }
</script>
```

| Output type | JSON item |
| --- | --- |
| `number` | Finite number |
| `integer` | Signed 32-bit integer |
| `boolean` | `true` or `false` |
| `text` | String, including a CSV string if the receiving definition expects one |
| `color` | Four RGBA integers from 0 through 255 |
| `point` | Three finite XYZ coordinates |

Set `access` to `item` for one typed value or `list` for an array of such values. Use `tree` with an array of branches such as `[{"path":[0,2],"items":[0.4,0.9]}]`. Paths contain 1–64 nonnegative 32-bit indices and must be unique within an output. A declaration supports 32 outputs, 1,024 branches per output, 10,000 values in total, and 256 KiB of UTF-8 JSON. Unknown types, invalid values, and duplicate IDs invalidate the entire declaration. A connected viewer waits until a valid declaration restores its output.

A placed image also exposes an **Image file** output when a Grasshopper viewer is loaded. Connect it directly to a Grasshopper text or file-path input. The receiver gets the local file path. Slate watches wired images in the background, and an overwrite or relink triggers a new solve. The timber study reads image luminance at each fin and height. Its embedded image supplies a standalone Grasshopper default.

The Grasshopper receiver can expose a contextual input or an unlocked typed parameter in an `RH_IN` group. Slate passes the typed data tree to that parameter's stable Grasshopper InstanceGuid. Compatible connections include integer to number and point to geometry. Grasshopper applies its normal item, list or tree matching at the destination.

A live, permitted Web component publishes values only from its bound page. During interaction Slate samples the latest complete declaration and coalesces changes. Local interactive dashboards can preserve their controls per placed component. Saving a wire preserves the endpoint UUIDs. An offscreen dashboard retains its last observed values while its browser is suspended. After reopening a workbook, Slate samples wired dashboards using spare browser slots. Visible portals retain priority. Opening the dashboard gives it the normal focused slot.


## Saving controller parameters

Add a stable schema UUID in `data-slate-controller` on the output script to make the page's editable parameters portable. Listen for `slate-controller-restore` **on that element**. Its `detail` is the saved JSON value, or `null` for a new controller. Restore the UI and republish the derived outputs. A user completing a drag, keyboard change or edit dispatches `slate-controller-edit` on the same element with the new JSON value in `detail`. Keep the snapshot within 8 KiB of UTF-8 JSON.

```js
element.addEventListener("slate-controller-restore", event => {
  applyControls(event.detail ?? initialControls);
  publishWeights();
});
function finishGesture() {
  element.dispatchEvent(new CustomEvent("slate-controller-edit", {detail: readControls()}));
}
```

Slate stores the parameters on the individual Web component when the user saves the workbook. Each duplicate can then be edited independently. Restoring the workbook on another computer, changing tabs, and waking an evicted component use the same saved state. Rebinding a component to a new page clears its prior parameters. The standalone page supplies its own initial controls.
