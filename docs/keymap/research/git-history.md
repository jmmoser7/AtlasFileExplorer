# Repository-history viewers — research input

> **Research input** for the repository Lens portal
> (`docs/keymap/contracts/portal-lens-repository.md`). Documents how existing
> tools draw and drive a commit DAG: GitKraken Desktop and the GitLens Commit
> Graph, GitHub's network graph, `git log --graph` itself, and the lane-layout
> algorithms that sit under all of them. Where a behaviour is a product choice
> we would not copy (anything that writes to the repository), that is called
> out in the *Slate adaptation* row rather than dropped, so the refusal is a
> decision and not an omission.

---

## Research methodology

Primary sources consulted:

| Source | URL |
|--------|-----|
| GitKraken Desktop — interface & commit graph | https://help.gitkraken.com/gitkraken-desktop/interface/ |
| GitKraken Desktop — hide and solo branches | https://help.gitkraken.com/gitkraken-desktop/hiding-and-soloing/ |
| GitKraken Desktop — branching and merging (pinning, smart visibility) | https://help.gitkraken.com/gitkraken-desktop/branching-and-merging/ |
| GitLens — Commit Graph (columns, scroll markers, minimap) | https://help.gitkraken.com/gitlens/gl-commit-graph/ |
| GitHub Docs — understanding connections between repositories (network graph, forks list) | https://docs.github.com/en/repositories/viewing-activity-and-data-for-your-repository/understanding-connections-between-repositories |
| GitHub Blog — "Say hello to the Network Graph Visualizer" | https://github.blog/news-insights/say-hello-to-the-network-graph-visualizer/ |
| git — `graph.h`, the line-by-line ASCII graph API | https://github.com/git/git/blob/master/graph.h |
| Azure DevOps Blog — commit-graph generations and topological order | https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/ |
| "Drawing a better git graph: from ASCII to orthogonal routing" | https://griffen.codes/post/better-git-graph-orthogonal-routing/ |
| Stack Overflow — how `git log --graph` distinguishes branches (first-parent rule) | https://stackoverflow.com/questions/67200056/how-can-git-log-graph-distinguish-between-branches |
| VisFork — visualising fork ecosystems (research tool, GitHub API) | https://jacobkrueger.github.io/assets/papers/Chen2025ForkVisTool.pdf |

---

## 1. The layout problem

Every tool in this space solves the same problem: a commit DAG has no natural
2D embedding, so one axis carries history order and the other carries
concurrency ("lanes", "swimlanes", "columns").

| Dimension | Observed behaviour |
|-----------|--------------------|
| **(a) History axis** | Desktop clients (GitKraken, GitLens, `gitk`, Sublime Merge) run history **vertically, newest at the top**, one row per commit. GitHub's network graph runs it **horizontally, oldest at the left**, and labels the rows with repository owners rather than branches. |
| **(b) Spacing** | Desktop clients space rows **topologically** — one row per commit regardless of when it happened. GitHub's network graph spaces commits along a **timeline** with date headings. The trade is legibility versus honesty about rhythm: topological hides the three-month gap, chronological hides forty commits made in one afternoon behind each other. |
| **(c) Lane assignment** | A single forward pass over commits in topological order, maintaining a set of open lanes, each holding the hash the lane is currently heading toward. A commit takes the lane that was waiting for it, or the first free lane. Its **first parent inherits the lane**; further parents (a merge's second parent) take their own lane. Lanes are not compacted while scrolling, so a vertical line stays where the eye left it. |
| **(d) First-parent convention** | `git merge` records the branch merged *into* as the first parent. `git log --graph` leans on that: the first parent continues straight, other parents angle off. It is a convention rather than a guarantee — a hand-crafted commit can violate it — but every mainstream client relies on it, and long-lived branches read as straight lines because of it. |
| **(e) Colour** | Colour is per-lane, allocated round-robin, and **stable for the life of the lane** in the better implementations. Colour that changes when a branch shifts column is the single most-reported legibility complaint against naive layouts. |
| **(f) Ordering stability** | Git's own topological sort (Kahn's algorithm over in-degrees, generation numbers to make it incremental) exists to guarantee that a commit is always drawn before its parents, and to minimise crossings by preferring the merged topic branch before continuing first-parent history. |

**Slate adaptation.** Time is the long axis on a landscape board frame, so the
history axis runs **horizontally, oldest left → newest right** — the direction
the Lens already uses for dependency depth (foundations left, apps right), and
the direction GitHub's network graph chose for the same reason. Lanes stack
downward with the trunk pinned to lane 0. Spacing is a journaled query knob
(`axis: Topological | Chronological`) because the two answers serve different
questions and the contract should not pretend one of them is wrong.

## 2. Focus, highlight, and dimming

| Dimension | Observed behaviour |
|-----------|--------------------|
| **(a) Hover a commit** | Tooltip / detail popover: hash, author, date, subject. GitKraken additionally resolves the commit's **nearest containing branch** ("ghost branch") so an unlabelled commit still says where it lives. |
| **(b) Hover a ref** | Hovering a branch highlights every commit reachable from it and **fades the unrelated ones**. GitKraken makes this toggleable in preferences, which tells you it is strong enough to be annoying at times. |
| **(c) Click a commit** | Selects it and fills a detail panel; nothing about the repository changes. GitHub opens the commit page in a new window instead, explicitly so that the reader keeps their place in the graph. |
| **(d) Reduce clutter** | GitKraken offers **hide** (drop refs from the view) and **solo** (show only the explicitly soloed refs), plus "smart branch visibility" (checked-out branch, its target, and their upstreams only), and **pinning** a long-lived branch to a fixed side. All are view state; none touch the repository. |
| **(e) Overview** | GitLens adds a minimap and scroll markers for HEAD, search hits, and refs — an admission that a tall graph is hard to navigate by scrollbar alone. |

**Slate adaptation.** Hover-to-highlight and click-to-focus map onto the
Lens's existing focus model (neighbours keep full alpha, everything else dims
to 25%) — one dimming convention across both analysis portals. Hide and solo
become the query's ref filter, which **is** journaled, because a board authored
to show one branch's story must show that branch's story when someone else
opens it. Hover, focus, and search are per-peer presence and are not. Slate
needs no minimap of its own: the portal lives on the canvas, and the canvas
has one.

## 3. The fork question

GitHub's network graph is the only widely used view of *forking*, and it is
worth being precise about what it actually shows.

- It draws "the branch history of the entire repository network, including
  fork branches" — up to the 100 most recently pushed-to branches, as a
  timeline with the branch owner in the first column.
- **Each commit appears exactly once** in the whole picture. A fork that
  pulled from upstream does not redraw the shared commits; only commits unique
  to that fork appear on its row. That property falls out of commit identity
  (the SHA), not out of any clever layout.
- Clicking a username **re-roots** the graph on that person's repository,
  which changes which commits count as "unique" to everyone else.
- The data comes from the host, not from git: a clone knows nothing about who
  forked it. Research tooling that wants the fork ecosystem (VisFork) does it
  with an authenticated GitHub API token, paging forks, branches, and commits
  per date range.

**Slate adaptation.** A local clone can honestly show: every local ref, every
ref of every configured remote, and the DAG that connects them — which is the
fork surface the machine actually has, and for a multi-remote clone
(`origin` + `upstream` + a colleague) it is exactly GitHub's picture minus the
strangers. The hosted fork network is *not* derivable from the clone, and
inventing it is barred by the false-affordance register (row 4, hallucinated
analysis graphs). It stays an optional out-of-process enrichment whose absence
renders as `Unknown` and whose presence is never required — an account
requirement inside the tool would breach Article I.4.

## 4. What these tools do that this portal will not

Every desktop client in this space is a **git client**: the graph is a control
surface for staging, committing, checking out, branching, rebasing,
cherry-picking, resolving conflicts, and pushing. GitKraken's right-click menu
on a commit is most of its product.

**Slate adaptation.** None of it. The portal is a reading instrument over
history, and writing to the repository is barred twice over — Article IX.5
(write-back is per-action, human-directed, and never available to an agent)
and the false-affordance register's row 5. The 10% that earns its place is:
*what shape did this history have, when did these branches diverge, and when
did they come back together.* Anything that mutates the repository is a
non-goal in the contract, listed so that it reads as a decision.

## 5. Performance notes worth stealing

- Lane layout is computed **once from the commit list and cached**; scrolling
  re-renders from the cache. The orthogonal-routing write-up singles this out
  as the change that made both the code and the picture faster to reason about.
- Git's own graph API is deliberately **line-by-line and incremental**
  (`graph_next_line`), because the full history of a large repository is not
  something you lay out eagerly.
- Generation numbers exist so that the topological walk can be advanced
  lazily rather than run to completion before the first row is printed.

**Slate adaptation.** Article II says this anyway (tessellate once, cache by
zoom bucket; heavy work async and generation-tagged), but it is reassuring
that the tools which live in this problem arrived at the same rules. The
contract's performance row commits to a windowed paint over the visible time
range and a default commit cap, so first meaningful paint does not scale with
repository age.
