# Linked Grasshopper pavilion

Open `Linked Grasshopper - Pavilion.slate` in Slate with Rhino 8 installed. Keep the five `.ghx` files beside the workbook. The definitions use built-in Grasshopper components and embedded C# for the simple geometry and sample data.

1. Select **01 Program** and open **Grasshopper inputs**. Change **Base width** to resize the pavilion. The number, list, and tree outputs propagate through the two branches to **04 Assemble**, then to **05 Pavilion**.
2. Right-click **Project label** on the left of **01 Program** and enter a new label. Its text travels through **03 Brief** to **04 Assemble** and **05 Pavilion**.
3. Right-click an output grip and choose **Bundle wires**. The two connections to each destination share a trunk. Choose **Unbundle wires** to restore the individual strokes.
4. In Rhino, open **02 Dimensions.ghx**, rename the numeric output parameter’s group or change the definition, and save. The viewer reloads the saved file. A renamed marker with the same identity retains its connection.
5. Use **Bake** on **05 Pavilion** to place the solved model on the canvas as a `.3dm`.

The default width is 6. The sample list is `6, 9, 12`. The tree contains path `{0;1}` with `6, 8` and path `{2;4}` with `11`. The default text is `Garden pavilion`.
