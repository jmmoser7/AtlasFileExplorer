# Parametric louvers

Open **Parametric louvers.slate** with the `controllers` and `definitions` folders beside it. Both frames contain a working input wire and an editable Grasshopper definition.

**Wave louvers.** Double-click the spline graph to interact with it. Drag points or curves, add intersections, and watch the white fins change. The HTML publishes a text field containing a sampled depth value for every fin and height. White fins.ghx reads the field and constructs the geometry.

**Timber relief.** The image card supplies a file path to Timber fins.ghx. The definition samples the luminance of the linked pixels to shape the timber fins. Replace the image to change the wall. In standalone Grasshopper, drop a picture onto the Image Sampler and save the GHX to embed it. The Slate image wire takes precedence when connected. Both routes were checked with black and white images. They produced identical geometry, with fin depths of 60 mm and 570 mm respectively.

The full set of supported values, stable output identifiers, and item, list and tree representations are documented in the [HTML dashboard output contract](../../docs/application-connectors/grasshopper-dashboard-outputs.md).
