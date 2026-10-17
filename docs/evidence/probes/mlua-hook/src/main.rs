use std::{cell::Cell, rc::Rc};
use mlua::{Error, HookTriggers, Lua};

fn main() -> mlua::Result<()> {
    let lua = Lua::new();
    lua.set_memory_limit(32 * 1024 * 1024)?;
    let trips = Rc::new(Cell::new(0usize));
    let hook_trips = Rc::clone(&trips);
    lua.set_hook(HookTriggers::new().every_nth_instruction(1_000), move |_, _| {
        hook_trips.set(hook_trips.get() + 1);
        Err(Error::runtime("instruction budget exceeded"))
    })?;
    // Only the inner loop is deliberately unbounded; each hook exits it.
    // The outer loop is finite so this probe terminates without a hostile worker.
    let result = lua.load(r#"
        local caught = 0
        for i = 1, 5 do
            local ok = pcall(function() while true do end end)
            if not ok then caught = caught + 1 end
        end
        return caught
    "#).eval::<u32>();
    lua.remove_hook();
    let caught = result?;
    assert_eq!(caught, 5);
    assert_eq!(trips.get(), 5);
    println!("runtime={}", lua.globals().get::<String>("_VERSION")?);
    println!("mlua_hook_errors={}, caught_by_pcall={caught}", trips.get());
    println!("Result: this embedding's ordinary hook errors do not force outer evaluation termination.");
    Ok(())
}
