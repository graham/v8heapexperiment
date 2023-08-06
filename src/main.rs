fn main() {
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

        // Heap doesn't continue to grow with this source
        // let source = "export let main = () => 'hello world'";

        // Reaches JS heap limit and panics.
        let source = "export let main = async () => { return 'hello world' }";

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

    let function_name = "main";

    // Keep track of any promises that which have not yet resolved.
    let mut pending_promises: Vec<v8::Global<v8::Promise>> = Vec::new();

    let mut iterations: u64 = 0;

    // If the following lines are within the loop, memory is cleared
    // and the heap is reset correctly.
    let mut handle = v8::HandleScope::new(&mut isolate);
    let global_template = v8::ObjectTemplate::new(&mut handle);
    let context = v8::Context::new_from_template(&mut handle, global_template);

    loop {
        let mut context_scope = v8::ContextScope::new(&mut handle, context);
        let module = v8::Local::new(&mut context_scope, global_module.clone());

        let ns = v8::Local::<v8::Object>::try_from(module.get_module_namespace()).unwrap();
        let name = v8::String::new(&mut context_scope, &function_name).unwrap();
        let func = ns.get(&mut context_scope, name.into()).unwrap();

        let handle_request_fn = v8::Local::<v8::Function>::try_from(func).unwrap();

        // Call a function that returns a promise, and store those promises
        // in our vector to be checked later for resolution.
        for _i in 0..5 {
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
        }

        pending_promises = pending_promises
            .into_iter()
            .filter(|p| {
                let lcl: v8::Local<'_, v8::Promise> = v8::Local::new(&mut context_scope, p);
                if lcl.state() == v8::PromiseState::Fulfilled {
                    let s = lcl
                        .result(&mut context_scope)
                        .to_string(&mut context_scope)
                        .unwrap()
                        .to_rust_string_lossy(&mut context_scope);
                    if s.eq("hello world") == false {
                        panic!("Unexpected result");
                    }
                    return false;
                } else {
                    return true;
                }
            })
            .collect::<Vec<_>>();

        iterations += 1;
        context_scope.perform_microtask_checkpoint();

        if iterations % 10000 == 0 {
            let mut s = v8::HeapStatistics::default();
            context_scope.get_heap_statistics(&mut s);
            println!("Used Heap Size: {}", s.used_heap_size());
            println!("Total Heap Size: {}", s.total_heap_size());
            println!("Unresolved Promises: {}", pending_promises.len());
            println!("----------------\n");
        }
    }
}

// These are helper functions for compiling modules, as far as I can tell
// these are not the issue.

pub fn resolve_module_imports<'a, 'b>(
    context: v8::Local<'a, v8::Context>,
    specifier: v8::Local<'a, v8::String>,
    _import_assertions: v8::Local<'a, v8::FixedArray>,
    _referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'b, v8::Module>>
where
    'a: 'b,
{
    None
}

pub fn create_module<'s>(
    scope: &mut v8::HandleScope<'s, v8::Context>,
    source: &str,
    code_cache: Option<v8::UniqueRef<v8::CachedData>>,
    options: v8::script_compiler::CompileOptions,
) -> Result<v8::Local<'s, v8::Module>, String> {
    let source = v8::String::new(scope, source).unwrap();
    let resource_name = v8::String::new(scope, "<resource>").unwrap();
    let source_map_url = v8::undefined(scope);
    let script_origin = v8::ScriptOrigin::new(
        scope,
        resource_name.into(),
        0,
        0,
        false,
        0,
        source_map_url.into(),
        false,
        false,
        true,
    );
    let has_cache = code_cache.is_some();
    let source = match code_cache {
        Some(x) => {
            v8::script_compiler::Source::new_with_cached_data(source, Some(&script_origin), x)
        }
        None => v8::script_compiler::Source::new(source, Some(&script_origin)),
    };

    let mut try_catch = v8::TryCatch::new(scope);

    let module = v8::script_compiler::compile_module2(
        &mut try_catch,
        source,
        options,
        v8::script_compiler::NoCacheReason::NoReason,
    );

    if try_catch.has_caught() {
        let exception = try_catch.exception().unwrap();

        let exception_string = exception
            .to_string(&mut try_catch)
            .unwrap()
            .to_rust_string_lossy(&mut try_catch);

        let errstring = format!("Exception: {}\n\n", exception_string);

        return Err(errstring);
    }

    Ok(module.unwrap())
}
