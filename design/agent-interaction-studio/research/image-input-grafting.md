# Image input grafting — research and implementation specification

Status: researched 30 October 2026; browser interaction is a draft, not a native implementation. The explicit owner request supplies the visual rules below. No new wire colors are approved.

## What Grasshopper actually does

Graft adds a branch level and places each item in a separate branch. It changes data organization; it is not itself a concurrency switch. A singleton can be repeated across branches. Flatten instead joins branch contents into one list. Branch structure, order and identifiers matter when matching inputs. [McNeel: Advanced Data Structures, sections 3.3 and 3.5.3–3.5.4](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/advanced-data-structures/).

Grasshopper's default longest matching repeats the last item of a shorter list. Shortest matching discards unmatched items. Cross reference makes all combinations and multiplies result counts; it must not be confused with ordinary grafting. Grasshopper also communicates item/list/tree differences visually on wires. [McNeel: Introduction to Data Structures, sections 2.1 and 2.4](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/data-structures/).

Parallel execution is a separate implementation capability. McNeel documents multi-threaded solving for specific supported components, with distinct enabled/disabled modes. Consequently, grafting establishes independent work items but does not guarantee they execute simultaneously. [McNeel: Multi-threaded components](https://developer.rhino3d.com/en/guides/grasshopper/multi-treaded-components/).

## Slate adaptation — specified behavior

Right-click the Primary image, Text or Style image input to expose **Graft inputs**. Each input owns its setting. In this authoring Studio, right-click continues to open the target's Right-click Menu Studio; select Graft inputs and use Try selected action to exercise it. Right-clicking a connected source image or text also targets its receiver input. Existing image-body menu edits remain intact.

Ungrafted inputs remain a shared bundle of references. Grafting splits the ordered incoming references into independent branches. The initial Studio interpretation uses longest matching across grafted channels; ungrafted channels are shared with each branch. This is a deliberate Slate application rule, not a full Grasshopper tree interpreter. It avoids silently creating a Cartesian product when two channels are grafted. Full path-based tree matching or an explicit all-combinations mode requires a later reviewed design.

| Inputs | Result jobs |
|---|---|
| Primary A; styles X and Y ungrafted | One job using A with both styles |
| Primary A; styles X and Y grafted | Two jobs: A+X, A+Y |
| Grafted primaries A,B; grafted styles X,Y | Two jobs: A+X, B+Y |
| Grafted primaries A,B,C; grafted styles X,Y | Three jobs: A+X, B+Y, C+Y |
| Grafted text P,Q; shared image A and style X | Two jobs: A+X+P, A+X+Q |
| Empty grafted channel | No jobs; identify the empty input |

Count applies per branch. For two grafted styles and Count 3, six output images belong to the same image panel. Preserve stable branch order even when completion order differs. Each result must retain source IDs, branch index, prompt, model configuration, seed and source revision so it can be explained or regenerated independently.

## Port presentation

Retain the established input colors, circle width, offsets and edge fillets. A grafted input grows vertically into a capsule. Wire pitch is 4 world units. Capsule height is 12 + 4 × (min(incoming count,4) − 1), with a minimum of 12 and a maximum of 24. Center the stack about the original port datum; do not shift other handles. The fifth and later wires share the fourth terminal position; the capsule does not continue growing. The same rule applies to primary, text and style inputs. Turning graft off restores the circle and shared endpoint. Hidden handles do not leave floating dots; wires still terminate at the image edge.

## Native implementation obligations

The browser computes jobs and simulated results only. It makes no generation requests. Implement a pure job-planning owner before adding provider concurrency. Snapshot all inputs and inherited history once at submission. Each job receives an independent copy so one style variation cannot contaminate another's conversation. Combine output references, not mutable agent sessions. Retain branch/result provenance when branching downstream.

Run jobs through the existing provider scheduler with bounded concurrency and cancellation; honor provider-specific limits and retry policies. Report queued/running/completed/failed counts, preserve completed outputs on partial failure, and retry failed branches independently. Do not block the UI thread. Do not assume a provider's multi-image Count option substitutes for separate style-conditioned requests. Validate compatibility of each input with the selected image model before submission.

A full Cartesian product is not enabled in this pass. If added later, expose its result count before submission and make it an explicit matching choice. This specification does not amend native fill, stroke, crop, layer menus, palette colors or unrelated graph behavior.

## Review checks

Two styles create two results in one panel; multiple grafted channels use the documented deterministic matching; empty grafts create none; Count multiplies branch count; stack geometry stops at four wires. Disconnecting one style changes only the next submission. Past results remain available. Tests cover matching and cap math. Browser review verified the two-style capsule and two additional simulated outputs.
