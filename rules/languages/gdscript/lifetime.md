# GDScript standard: Lifetime

Part of the [GDScript standard](README.md). Lifetime, concurrency and the editor.

## Lifetime, concurrency, and the editor

### Ownership of nodes and objects

*Default.* Each node has one lifetime owner. Its creator owns it until it enters the tree, where its parent owns it. The creator frees a node that was never added or was removed with `remove_child`, which does not free it. Forgotten orphans appear in the engine's leaked-instance report at exit. `Node.owner` instead determines which nodes a scene saves. Set it only for nodes belonging in a saved scene.

Use `queue_free()` by default. It removes the node at frame end, after callbacks that hold it. `free()` is immediate and applies only to objects nothing else can hold. Freeing an object during signal emission or a call reports `Object is locked and can't be freed` and leaves it alive (measured). Test freed-node references as the gaps table specifies. A `RefCounted` cycle, such as records referring back to each other, is never freed. Use `weakref()` on one side or clear the ordinary reference during teardown.

Ref: `Node.queue_free`, `Node.owner`, `Object.free`, and `RefCounted` in the class reference; the manual's "Nodes and scene instances".

**Checked by:** the engine's leak report at exit and the error filter (a locked free); review.

### Coroutines

*Default.* A function containing `await` is a coroutine. Calling it without `await` runs it detached. `missing_await` at Error refuses that call unless it has `@warning_ignore("missing_await")` and a comment explaining the detachment (measured). A call through `Callable.call()` escapes that check. Name the detached coroutine's owner in the comment. The owner must end the coroutine or outlive it.

Across an `await`, the tree, collaborators, and current request may change. Before using them after resumption, check that they still exist with `is_instance_valid` and belong to the same request. A coroutine waiting for a freed object's signal never resumes, and nothing reports the failure (measured). Await only signals from objects that outlive the coroutine, unless whatever frees the object also ends the coroutine. A script error after `await` abandons the coroutine without reaching a caller. Only the error filter detects it.

Initialisation runs member defaults, `_init`, exported values, `@onready`, and `_ready`, in that order. An export read in `_init` still has its default. In an `extends SceneTree` script's `_init`, `root` exists but is not yet inside the tree (measured). Initialisers do not run setters. Later assignments do, even inside the class.

Ref: the GDScript reference, "Awaiting signals or coroutines" and "Initialization order"; the manual's "Godot notifications".

**Checked by:** the engine (`missing_await`, `redundant_await`, `get_node_default_without_onready`, `onready_with_export`); the error filter; review (detached owners, checks after resuming).

### Threads

*Always.* A `Thread` or `WorkerThreadPool` worker takes plain inputs and returns a plain result. It touches no active-tree node, rendering or GPU state, or resource or object the main thread may hold. Exceptions require the manual's "Thread-safe APIs" to declare the API safe. The main thread collects the result, including refusals, by polling or through `call_deferred`. Only the main thread publishes it to the tree. An owner waits for every started thread or task, no later than when the owner leaves the tree. A worker never waits for main-thread work, preventing deadlock during that wait. Treat worker failures, including a failed start, as refusals.

For example, in tdollhouse: character refresh reads and decodes each source on one `Thread`. Scene and GPU construction remain on the main thread. A failed `Thread.start` becomes a refused input. `_exit_tree` waits for the thread.

Ref: the manual's "Using multiple threads" and "Thread-safe APIs".

**Checked by:** review.

### Static state

*Default.* A `static var` lasts as long as its script. In 4.7, that is the whole process, regardless of `@static_unload`. Checks run in one process share that state. Each static variable has an owner that documents its reset time in `##`. A check that changes a static variable restores it. Static initialisers run on script load (measured), including compile-check loading. They must need no running game, node, file write, or possibly absent engine singleton. A Godot 5 proposal allows only constant static initialisers and removes `_static_init`. Keep static initialisers and `_static_init` small.

Ref: the GDScript reference, "Static variables" and "Static constructor"; godot-proposals #14652.

**Checked by:** review.

### Tool scripts

*Always.* A `@tool` script runs in the editor. Its `_init`, setters, `_ready`, and `_process`, and those of its resources, can run when a scene opens or a resource is inspected. Every subclass is also `@tool`, enforced by `missing_tool`. Guard game-only behaviour with `Engine.is_editor_hint()`. Never write a saved scene, resource, or file merely because the script was opened or inspected. Never free or reparent nodes owned by the edited scene. Create permitted editor changes, such as previews or unsaved generated children, without `owner`, so they are not saved. Tool-script errors appear only in the editor unless a check loads the script too.

Ref: the manual's "Running code in the editor".

**Checked by:** the engine (`missing_tool`); review.
