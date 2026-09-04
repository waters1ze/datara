use cranelift_codegen::isa::TargetIsa;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;
use std::time::Instant;

unsafe extern "C" {
    pub fn datara_rt_out_int(v: i64);
    pub fn datara_rt_out_bool(v: i64);
    pub fn datara_rt_bool_to_str(v: i64) -> *const c_char;
    pub fn datara_rt_out_float(v: f64);
    pub fn datara_rt_float_to_str(v: f64) -> *const c_char;
    pub fn datara_rt_out_str(s: *const c_char);
    pub fn datara_rt_out_dec64(v: i64);
    pub fn datara_rt_err(s: *const c_char);
    pub fn datara_rt_exit(code: i32);
    pub fn datara_rt_input(prompt: *const c_char) -> *const c_char;
    pub fn datara_rt_input_int(prompt: *const c_char) -> i64;
    pub fn datara_rt_input_float(prompt: *const c_char) -> f64;

    pub fn datara_rt_print_str(s: *const c_char);
    pub fn datara_rt_print_int(v: i64);
    pub fn datara_rt_print_float(v: f64);
    pub fn datara_rt_print_bool(v: i64);
    pub fn datara_rt_print_space();
    pub fn datara_rt_print_newline();
    pub fn datara_rt_flush();
    pub fn datara_rt_print_list(list: *mut ());
    pub fn datara_rt_println(s: *const c_char);
    pub fn datara_rt_print(s: *const c_char);
    pub fn datara_rt_eprintln(s: *const c_char);
    pub fn datara_rt_panic(s: *const c_char);
    pub fn datara_rt_assert(cond: i64, msg: *const c_char);
    pub fn datara_rt_len(s: *const c_char) -> i64;

    pub fn datara_rt_set_capture(enable: i32);
    pub fn datara_rt_get_capture() -> *const c_char;
    pub fn datara_rt_clear_capture();

    pub fn datara_rt_int_to_str(v: i64) -> *const c_char;
    pub fn datara_rt_str_concat(a: *const c_char, b: *const c_char) -> *const c_char;
    pub fn datara_rt_str_concat_3(
        a: *const c_char,
        b: *const c_char,
        c: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_str_concat_4(
        a: *const c_char,
        b: *const c_char,
        c: *const c_char,
        d: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_str_concat_5(
        a: *const c_char,
        b: *const c_char,
        c: *const c_char,
        d: *const c_char,
        e: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_format_str_i64_str_i64(
        s1: *const c_char,
        n1: i64,
        s2: *const c_char,
        n2: i64,
    ) -> *const c_char;
    pub fn datara_rt_str_eq(a: *const c_char, b: *const c_char) -> i64;
    pub fn datara_rt_str_len(s: *const c_char) -> i64;
    pub fn datara_rt_str_contains(s: *const c_char, sub: *const c_char) -> i64;
    pub fn datara_rt_str_starts_with(s: *const c_char, pre: *const c_char) -> i64;
    pub fn datara_rt_str_ends_with(s: *const c_char, suf: *const c_char) -> i64;
    pub fn datara_rt_str_index_of(s: *const c_char, sub: *const c_char) -> i64;
    pub fn datara_rt_str_trim(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_to_int(s: *const c_char) -> i64;
    pub fn datara_rt_str_to_float(s: *const c_char) -> f64;
    pub fn datara_rt_str_substring(s: *const c_char, start: i64, len: i64) -> *const c_char;
    pub fn datara_rt_str_char_at(s: *const c_char, idx: i64) -> *const c_char;

    pub fn datara_rt_file_read(path: *const c_char) -> *const c_char;
    pub fn datara_rt_file_write(path: *const c_char, content: *const c_char) -> i64;
    pub fn datara_rt_file_append(path: *const c_char, content: *const c_char) -> i64;
    pub fn datara_rt_file_exists(path: *const c_char) -> i64;

    pub fn datara_rt_sleep(ms: i64);
    pub fn datara_rt_now_ms() -> i64;
    pub fn datara_rt_now_unix_ms() -> i64;
    pub fn datara_rt_now_precise_ms() -> i64;
    pub fn now_ms() -> i64;
    pub fn datara_rt_env_get(key: *const c_char) -> *const c_char;
    pub fn datara_rt_set_args(argc: i32, argv: *const *const c_char);
    pub fn datara_rt_args_count() -> i64;
    pub fn datara_rt_args_get(index: i64) -> *const c_char;

    pub fn datara_rt_list_create(cap: i64) -> *mut i64;
    pub fn datara_rt_list_create_capacity(cap: i64) -> *mut i64;
    pub fn datara_rt_list_create_repeat(len: i64, val: i64) -> *mut i64;
    pub fn datara_rt_list_append(list: *mut i64, val: i64) -> *mut i64;
    pub fn datara_rt_list_len(list: *mut i64) -> i64;
    pub fn datara_rt_list_get(list: *mut i64, idx: i64) -> i64;
    pub fn datara_rt_list_set(list: *mut i64, idx: i64, val: i64) -> i64;
    pub fn datara_rt_list_pop(list: *mut i64) -> i64;
    pub fn datara_rt_range_str(start: i64, end: i64) -> *const c_char;
    pub fn datara_rt_map_create() -> *mut ();
    pub fn datara_rt_map_create_2(k1: i64, v1: i64, k2: i64, v2: i64) -> *mut ();
    pub fn datara_rt_map_insert(map: *mut i64, key: *const c_char, val: i64) -> *mut i64;
    pub fn datara_rt_map_get(map: *mut i64, key: *const c_char) -> i64;
    pub fn datara_rt_map_free(map: *mut ());

    pub fn datara_rt_socket_create(is_tcp: i64) -> i64;
    pub fn datara_rt_socket_bind(sock: i64, host: *const c_char, port: i64) -> i64;
    pub fn datara_rt_socket_listen(sock: i64, backlog: i64) -> i64;
    pub fn datara_rt_socket_accept(sock: i64) -> i64;
    pub fn datara_rt_socket_connect(sock: i64, host: *const c_char, port: i64) -> i64;
    pub fn datara_rt_socket_send(sock: i64, data: *const c_char) -> i64;
    pub fn datara_rt_socket_recv(sock: i64, max_bytes: i64) -> *const c_char;
    pub fn datara_rt_socket_close(sock: i64);
    pub fn datara_rt_http_get() -> *const c_char;

    pub fn datara_rt_sha256(input: *const c_char) -> *const c_char;
    pub fn datara_rt_base64_encode(input: *const c_char) -> *const c_char;
    pub fn datara_rt_base64_decode(input: *const c_char) -> *const c_char;
    pub fn datara_rt_random_bytes(buf: *mut u8, len: i64) -> i64;
    pub fn datara_rt_uuid_v4() -> *const c_char;

    pub fn datara_rt_dialog_info(title: *const c_char, msg: *const c_char) -> i64;
    pub fn datara_rt_dialog_alert(title: *const c_char, msg: *const c_char) -> i64;
    pub fn datara_rt_dialog_confirm(title: *const c_char, msg: *const c_char) -> i64;

    pub fn datara_rt_system(cmd: *const c_char) -> i64;
    pub fn datara_rt_exec(cmd: *const c_char) -> *const c_char;

    pub fn datara_rt_math_sqrt(x: f64) -> f64;
    pub fn datara_rt_math_pow(base: f64, exp: f64) -> f64;
    pub fn datara_rt_math_abs(x: f64) -> f64;
    pub fn datara_rt_math_sin(x: f64) -> f64;
    pub fn datara_rt_math_cos(x: f64) -> f64;
    pub fn datara_rt_math_tan(x: f64) -> f64;
    pub fn datara_rt_math_floor(x: f64) -> f64;
    pub fn datara_rt_math_ceil(x: f64) -> f64;
    pub fn datara_rt_math_round(x: f64) -> f64;
    pub fn datara_rt_math_min(a: f64, b: f64) -> f64;
    pub fn datara_rt_math_max(a: f64, b: f64) -> f64;
    pub fn datara_rt_math_hypot(a: f64, b: f64) -> f64;
    pub fn datara_rt_math_min_int(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_max_int(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_abs_int(x: i64) -> i64;
    pub fn datara_rt_math_ctz(x: i64) -> i64;
    pub fn datara_rt_math_shr(v: i64, s: i64) -> i64;
    pub fn datara_rt_math_shl(v: i64, s: i64) -> i64;
    pub fn datara_rt_math_xor(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_and(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_or(a: i64, b: i64) -> i64;

    pub fn datara_rt_arena_alloc(bytes: i64) -> *mut ();
    pub fn datara_rt_arena_checkpoint() -> i64;
    pub fn datara_rt_arena_reset(saved_top: i64);
    pub fn datara_rt_free(ptr: *mut ());
    pub fn datara_rt_str_free(s: *const c_char);
    pub fn datara_rt_list_free(list: *mut ());

    fn malloc(size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
}

pub fn register_runtime_symbols(builder: &mut JITBuilder) {
    macro_rules! reg {
        ($sym:expr, $func:ident) => {
            builder.symbol($sym, $func as *const u8);
        };
    }

    reg!("malloc", malloc);
    reg!("free", free);

    reg!("datara_rt_out_int", datara_rt_out_int);
    reg!("datara_rt_out_bool", datara_rt_out_bool);
    reg!("datara_rt_bool_to_str", datara_rt_bool_to_str);
    reg!("datara_rt_out_float", datara_rt_out_float);
    reg!("datara_rt_float_to_str", datara_rt_float_to_str);
    reg!("datara_rt_out_str", datara_rt_out_str);
    reg!("datara_rt_out_dec64", datara_rt_out_dec64);
    reg!("datara_rt_err", datara_rt_err);
    reg!("datara_rt_exit", datara_rt_exit);
    reg!("datara_rt_input", datara_rt_input);
    reg!("input", datara_rt_input);
    reg!("read_line", datara_rt_input);
    reg!("datara_rt_input_int", datara_rt_input_int);
    reg!("datara_rt_input_float", datara_rt_input_float);

    reg!("datara_rt_print_str", datara_rt_print_str);
    reg!("datara_rt_print_int", datara_rt_print_int);
    reg!("datara_rt_print_float", datara_rt_print_float);
    reg!("datara_rt_print_bool", datara_rt_print_bool);
    reg!("datara_rt_print_space", datara_rt_print_space);
    reg!("datara_rt_print_newline", datara_rt_print_newline);
    reg!("datara_rt_flush", datara_rt_flush);
    reg!("datara_rt_print_list", datara_rt_print_list);
    reg!("datara_rt_println", datara_rt_println);
    reg!("println", datara_rt_println);
    reg!("datara_rt_print", datara_rt_print);
    reg!("print", datara_rt_print);
    reg!("datara_rt_eprintln", datara_rt_eprintln);
    reg!("eprintln", datara_rt_eprintln);
    reg!("datara_rt_panic", datara_rt_panic);
    reg!("panic", datara_rt_panic);
    reg!("datara_rt_assert", datara_rt_assert);
    reg!("assert", datara_rt_assert);
    reg!("datara_rt_len", datara_rt_len);

    reg!("datara_rt_set_capture", datara_rt_set_capture);
    reg!("datara_rt_get_capture", datara_rt_get_capture);
    reg!("datara_rt_clear_capture", datara_rt_clear_capture);

    reg!("datara_rt_int_to_str", datara_rt_int_to_str);
    reg!("int_to_str", datara_rt_int_to_str);
    reg!("datara_rt_str_concat", datara_rt_str_concat);
    reg!("str_concat", datara_rt_str_concat);
    reg!("datara_rt_str_concat_3", datara_rt_str_concat_3);
    reg!("datara_rt_str_concat_4", datara_rt_str_concat_4);
    reg!("datara_rt_str_concat_5", datara_rt_str_concat_5);
    reg!(
        "datara_rt_format_str_i64_str_i64",
        datara_rt_format_str_i64_str_i64
    );
    reg!("datara_rt_str_eq", datara_rt_str_eq);
    reg!("str_eq", datara_rt_str_eq);
    reg!("datara_rt_str_len", datara_rt_str_len);
    reg!("str_len", datara_rt_str_len);
    reg!("len", datara_rt_str_len);
    reg!("datara_rt_str_contains", datara_rt_str_contains);
    reg!("str_contains", datara_rt_str_contains);
    reg!("datara_rt_str_starts_with", datara_rt_str_starts_with);
    reg!("str_starts_with", datara_rt_str_starts_with);
    reg!("datara_rt_str_ends_with", datara_rt_str_ends_with);
    reg!("str_ends_with", datara_rt_str_ends_with);
    reg!("datara_rt_str_index_of", datara_rt_str_index_of);
    reg!("str_index_of", datara_rt_str_index_of);
    reg!("datara_rt_str_trim", datara_rt_str_trim);
    reg!("str_trim", datara_rt_str_trim);
    reg!("datara_rt_str_to_int", datara_rt_str_to_int);
    reg!("str_to_int", datara_rt_str_to_int);
    reg!("datara_rt_str_to_float", datara_rt_str_to_float);
    reg!("str_to_float", datara_rt_str_to_float);
    reg!("datara_rt_str_substring", datara_rt_str_substring);
    reg!("str_substring", datara_rt_str_substring);
    reg!("datara_rt_str_char_at", datara_rt_str_char_at);
    reg!("str_char_at", datara_rt_str_char_at);

    reg!("datara_rt_file_read", datara_rt_file_read);
    reg!("file_read", datara_rt_file_read);
    reg!("read", datara_rt_file_read);
    reg!("datara_rt_file_write", datara_rt_file_write);
    reg!("file_write", datara_rt_file_write);
    reg!("write", datara_rt_file_write);
    reg!("datara_rt_file_append", datara_rt_file_append);
    reg!("file_append", datara_rt_file_append);
    reg!("append", datara_rt_file_append);
    reg!("datara_rt_file_exists", datara_rt_file_exists);
    reg!("file_exists", datara_rt_file_exists);
    reg!("exists", datara_rt_file_exists);

    reg!("datara_rt_sleep", datara_rt_sleep);
    reg!("sleep", datara_rt_sleep);
    reg!("datara_rt_now_ms", datara_rt_now_ms);
    reg!("now_ms", datara_rt_now_ms);
    reg!("now", datara_rt_now_ms);
    reg!("datara_rt_now_unix_ms", datara_rt_now_unix_ms);
    reg!("datara_rt_now_precise_ms", datara_rt_now_precise_ms);
    reg!("now_precise_ms", datara_rt_now_precise_ms);
    reg!("datara_rt_env_get", datara_rt_env_get);
    reg!("env_get", datara_rt_env_get);
    reg!("datara_rt_set_args", datara_rt_set_args);
    reg!("datara_rt_args_count", datara_rt_args_count);
    reg!("args_count", datara_rt_args_count);
    reg!("datara_rt_args_get", datara_rt_args_get);
    reg!("args_get", datara_rt_args_get);

    reg!("datara_rt_list_create", datara_rt_list_create);
    reg!(
        "datara_rt_list_create_capacity",
        datara_rt_list_create_capacity
    );
    reg!("datara_rt_list_create_repeat", datara_rt_list_create_repeat);
    reg!("datara_rt_list_append", datara_rt_list_append);
    reg!("datara_rt_list_len", datara_rt_list_len);
    reg!("datara_rt_list_get", datara_rt_list_get);
    reg!("datara_rt_list_set", datara_rt_list_set);
    reg!("datara_rt_list_pop", datara_rt_list_pop);
    reg!("datara_rt_range_str", datara_rt_range_str);
    reg!("datara_rt_map_create", datara_rt_map_create);
    reg!("datara_rt_map_create_2", datara_rt_map_create_2);
    reg!("datara_rt_map_insert", datara_rt_map_insert);
    reg!("datara_rt_map_get", datara_rt_map_get);
    reg!("datara_rt_map_free", datara_rt_map_free);

    reg!("datara_rt_socket_create", datara_rt_socket_create);
    reg!("socket_create", datara_rt_socket_create);
    reg!("datara_rt_socket_bind", datara_rt_socket_bind);
    reg!("socket_bind", datara_rt_socket_bind);
    reg!("datara_rt_socket_listen", datara_rt_socket_listen);
    reg!("socket_listen", datara_rt_socket_listen);
    reg!("datara_rt_socket_accept", datara_rt_socket_accept);
    reg!("socket_accept", datara_rt_socket_accept);
    reg!("datara_rt_socket_connect", datara_rt_socket_connect);
    reg!("socket_connect", datara_rt_socket_connect);
    reg!("datara_rt_socket_send", datara_rt_socket_send);
    reg!("socket_send", datara_rt_socket_send);
    reg!("datara_rt_socket_recv", datara_rt_socket_recv);
    reg!("socket_recv", datara_rt_socket_recv);
    reg!("datara_rt_socket_close", datara_rt_socket_close);
    reg!("socket_close", datara_rt_socket_close);
    reg!("datara_rt_http_get", datara_rt_http_get);

    reg!("datara_rt_sha256", datara_rt_sha256);
    reg!("sha256", datara_rt_sha256);
    reg!("datara_rt_base64_encode", datara_rt_base64_encode);
    reg!("base64_encode", datara_rt_base64_encode);
    reg!("datara_rt_base64_decode", datara_rt_base64_decode);
    reg!("base64_decode", datara_rt_base64_decode);
    reg!("datara_rt_random_bytes", datara_rt_random_bytes);
    reg!("datara_rt_uuid_v4", datara_rt_uuid_v4);
    reg!("uuid_v4", datara_rt_uuid_v4);

    reg!("datara_rt_dialog_info", datara_rt_dialog_info);
    reg!("datara_rt_dialog_alert", datara_rt_dialog_alert);
    reg!("datara_rt_dialog_confirm", datara_rt_dialog_confirm);

    reg!("datara_rt_system", datara_rt_system);
    reg!("system", datara_rt_system);
    reg!("datara_rt_exec", datara_rt_exec);
    reg!("exec", datara_rt_exec);
    reg!("process_output", datara_rt_exec);

    reg!("datara_rt_math_sqrt", datara_rt_math_sqrt);
    reg!("math_sqrt", datara_rt_math_sqrt);
    reg!("datara_rt_math_pow", datara_rt_math_pow);
    reg!("math_pow", datara_rt_math_pow);
    reg!("datara_rt_math_abs", datara_rt_math_abs);
    reg!("math_abs", datara_rt_math_abs);
    reg!("datara_rt_math_sin", datara_rt_math_sin);
    reg!("math_sin", datara_rt_math_sin);
    reg!("datara_rt_math_cos", datara_rt_math_cos);
    reg!("math_cos", datara_rt_math_cos);
    reg!("datara_rt_math_tan", datara_rt_math_tan);
    reg!("math_tan", datara_rt_math_tan);
    reg!("datara_rt_math_floor", datara_rt_math_floor);
    reg!("math_floor", datara_rt_math_floor);
    reg!("datara_rt_math_ceil", datara_rt_math_ceil);
    reg!("math_ceil", datara_rt_math_ceil);
    reg!("datara_rt_math_round", datara_rt_math_round);
    reg!("math_round", datara_rt_math_round);
    reg!("datara_rt_math_min", datara_rt_math_min);
    reg!("math_min", datara_rt_math_min);
    reg!("datara_rt_math_max", datara_rt_math_max);
    reg!("math_max", datara_rt_math_max);
    reg!("datara_rt_math_hypot", datara_rt_math_hypot);
    reg!("math_hypot", datara_rt_math_hypot);
    reg!("datara_rt_math_min_int", datara_rt_math_min_int);
    reg!("math_min_int", datara_rt_math_min_int);
    reg!("datara_rt_math_max_int", datara_rt_math_max_int);
    reg!("math_max_int", datara_rt_math_max_int);
    reg!("datara_rt_math_abs_int", datara_rt_math_abs_int);
    reg!("math_abs_int", datara_rt_math_abs_int);
    reg!("datara_rt_math_ctz", datara_rt_math_ctz);
    reg!("math_ctz", datara_rt_math_ctz);
    reg!("datara_rt_math_shr", datara_rt_math_shr);
    reg!("math_shr", datara_rt_math_shr);
    reg!("datara_rt_math_shl", datara_rt_math_shl);
    reg!("math_shl", datara_rt_math_shl);
    reg!("datara_rt_math_xor", datara_rt_math_xor);
    reg!("math_xor", datara_rt_math_xor);
    reg!("datara_rt_math_and", datara_rt_math_and);
    reg!("math_and", datara_rt_math_and);
    reg!("datara_rt_math_or", datara_rt_math_or);
    reg!("math_or", datara_rt_math_or);

    reg!("datara_rt_arena_alloc", datara_rt_arena_alloc);
    reg!("datara_rt_arena_checkpoint", datara_rt_arena_checkpoint);
    reg!("datara_rt_arena_reset", datara_rt_arena_reset);
    reg!("datara_rt_free", datara_rt_free);
    reg!("datara_rt_str_free", datara_rt_str_free);
    reg!("datara_rt_list_free", datara_rt_list_free);
}

pub fn create_jit_module(isa: Arc<dyn TargetIsa>) -> Result<JITModule, String> {
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    register_runtime_symbols(&mut builder);
    Ok(JITModule::new(builder))
}

/// Executes JIT compiled entry point in process memory.
///
/// # Safety
///
/// `code_ptr` must point to valid executable machine code adhering to `extern "C" fn(i32, *const *const c_char) -> i32`.
pub unsafe fn run_jit_entry(
    code_ptr: *const u8,
    args: &[String],
    capture: bool,
) -> Result<(String, String, i32, u128), String> {
    if code_ptr.is_null() {
        return Err("JIT execution failed: entry function pointer is null".to_string());
    }

    type MainFn = unsafe extern "C" fn(i32, *const *const c_char) -> i32;
    let main_fn: MainFn = unsafe { std::mem::transmute(code_ptr) };

    let mut c_strings = Vec::with_capacity(args.len() + 1);
    c_strings.push(CString::new("forgen").unwrap_or_default());
    for a in args {
        c_strings.push(CString::new(a.as_str()).unwrap_or_default());
    }
    let c_ptrs: Vec<*const c_char> = c_strings.iter().map(|s| s.as_ptr()).collect();
    let argc = c_ptrs.len() as i32;
    let argv = c_ptrs.as_ptr();

    if capture {
        unsafe {
            datara_rt_clear_capture();
            datara_rt_set_capture(1);
        }
    }

    let start = Instant::now();
    let exit_code = unsafe { main_fn(argc, argv) };
    let duration = start.elapsed().as_millis();

    let stdout = if capture {
        unsafe {
            datara_rt_flush();
            datara_rt_set_capture(0);
            let ptr = datara_rt_get_capture();
            let out = if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };
            datara_rt_clear_capture();
            out
        }
    } else {
        unsafe {
            datara_rt_flush();
        }
        String::new()
    };

    Ok((stdout, String::new(), exit_code, duration))
}
