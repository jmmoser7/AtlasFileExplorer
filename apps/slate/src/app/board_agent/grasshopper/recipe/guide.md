For this new, empty definition, write exactly two UTF-8 text files in the request directory using your ordinary file-edit tool. Slate assembles the real editable Rhino 8 GHX after your turn. Use text editing alone. Finish after writing and checking the two files. Do not execute the geometry.

`parameters.json` declares the title and ordered scalar controls. A numeric input declaration looks like this.
```json
{"version":1,"title":"Parametric study","inputs":[{"key":"radius","name":"Radius","type":"number","min":1,"max":100,"default":20}]}
```
Choose the actual controls, units, ranges and defaults from the user's request. A `key` is a unique ASCII C# identifier beginning with a letter. `name` is the human-readable label. `number` and `integer` require numeric min, max and default. `boolean` and `text` require a matching JSON default and omit min/max. Up to 32 inputs are supported. Each input becomes an editable native control and a matching named, typed Grasshopper contextual item input with a stable InstanceGuid for Slate wiring.

`geometry.cs` contains **only the body** of the Grasshopper C# RunScript method. All inputs are already declared with their keys and types (`double`, `int`, `bool`, `string`). `System`, `System.Collections.Generic`, and `Rhino.Geometry` are imported. Assign one geometry or a collection of geometry, such as a `List<Brep>`, to the existing `model` output. Slate supplies the class, method signature and input declarations. Keep this file plain C# limited to the geometry calculation. Implement the user's actual geometry. For example a positive numeric radius can generate a sphere with `model = new Sphere(Point3d.Origin, radius).ToBrep();`.

If the request explicitly needs a different native component graph or types outside this scalar script format, you may instead write a complete editable definition.ghx by editing archive XML as text. The reviewed revision stays inert until the user chooses to open it.
