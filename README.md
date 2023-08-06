# v8heapexperiment

The following is a demo repository to recreate a heap memory issue I'm seeing in a larger project I am working on.

1. I have either found a memory leak

Or (and more likely)

2. I'm doing something wrong with promises in rusty_v8 and cannot figure it out myself.

------

If you are comfortable diving in, the full source is here: https://github.com/graham/v8heapexperiment/blob/main/src/main.rs

But I'll outline it here for better readability:

-----

I begin by initializing the v8 platform and compiling a module and storing it in a `v8::Global`.
```rust

    let platform = v8::new_default_platform(0, true).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let mut isolate = v8::Isolate::new(v8::CreateParams::default());

    // Compile a module with a single exported function `main`
    // scope it to global and store it in the global_module variable.
    let global_module = {
        let mut handle = v8::HandleScope::new(&mut isolate);

        let global = v8::ObjectTemplate::new(&mut handle);
        let context = v8::Context::new_from_template(&mut handle, global);
        let mut context_scope = v8::ContextScope::new(&mut handle, context);

        let source = SOURCE_CODE;

        let module = create_module(
            &mut context_scope,
            &source,
            None,
            v8::script_compiler::CompileOptions::NoCompileOptions,
        )
        .unwrap();

        module.instantiate_module(&mut context_scope, resolve_module_imports);
        module.evaluate(&mut context_scope).unwrap();

        let glob_module = v8::Global::new(&mut context_scope, module);

        glob_module
    };

```


### Async promise resolution appears to be the source of the issue

The value of `SOURCE CODE` is very important to this demonstration, so now is a good point to show when I see a runaway heap and when I do not.

when `SOURCE_CODE` is equal to `"export let main = () => 'hello world'";` there is no issue.

However, if the function `main` is async I see the v8 heap grow and eventually overflow: `"export let main = async () => { return 'hello world' }";`


### Keeping track of promises

I use the following vector to keep track of promises that have not yet been resolved:

```rust
    let mut pending_promises: Vec<v8::Global<v8::Promise>> = Vec::new();
```

When the function call returns a promise, I convert it to a `v8::Global` and store it in the vector:

```rust
            match handle_request_fn.call(&mut context_scope, ns.into(), &[]) {
                Some(value) => {
                    if value.is_promise() {
                        let promise = v8::Local::<'_, v8::Promise>::try_from(value)
                            .expect("Function did not return promise as expected.");
                        // Leaving this as a v8::Local appears to have no affect
                        // on the heap growing.
                        let p = v8::Global::new(&mut context_scope, promise);
                        pending_promises.push(p);
                    } else {
                    }
                }
                None => {}
            }
```

After a call to `perform_microtask_checkpoint` I iterate through the vector and check to see if any vector has been resovled.

I run this loop and even though all the promises have been resolved (and dropped from the vector), the heap continues to grow until it overflows.

If the `HandleScope` is dropped each iteration of the loop (loop starts here: https://github.com/graham/v8heapexperiment/blob/main/src/main.rs#L52 ) the heap is correctly trimmed as it is used.

Since the performance characteristics of re-creating the `HandleScope` ever iteration are very significant, I have to assume I'm somehow retaining a Promise somewhere in the stack.

As mentioned earlier, if this function does not generate a Promise, there is no heap issue.
