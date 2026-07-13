use beryl_backend::ThreadStartOptions;

use crate::dynamic_tools::beryl_dynamic_tool_specs;

pub fn beryl_thread_start_options() -> ThreadStartOptions {
    beryl_user_thread_start_options()
}

pub fn beryl_user_thread_start_options() -> ThreadStartOptions {
    ThreadStartOptions::persistent().with_dynamic_tools(beryl_dynamic_tool_specs())
}
