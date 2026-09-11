use cranelift_codegen::isa::TargetIsa;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;
use std::time::Instant;

use crate::runtime::scheduler::{
    datara_rt_schedule_cancel, datara_rt_schedule_run, datara_rt_time_delta_ms,
    datara_rt_time_precise_ms,
};

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
    pub fn datara_rt_print_backtrace();
    pub fn datara_rt_assert(cond: i64, msg: *const c_char);
    pub fn datara_rt_len(s: *const c_char) -> i64;
    pub fn datara_rt_pgo_hit_func(name: *const c_char);
    pub fn datara_rt_pgo_hit_branch(branch_id: *const c_char, taken: i64);
    pub fn datara_rt_pgo_hit_loop(loop_id: *const c_char, trip_count: i64);
    pub fn datara_rt_pgo_set_output_file(path: *const c_char);
    pub fn datara_rt_pgo_flush(path: *const c_char);
    pub fn datara_rt_pgo_reset();

    pub fn datara_rt_chase_lev_create(capacity: i64) -> *mut ();
    pub fn datara_rt_chase_lev_destroy(q: *mut ());
    pub fn datara_rt_chase_lev_push(q: *mut (), task_id: i64);
    pub fn datara_rt_chase_lev_pop(q: *mut ()) -> i64;
    pub fn datara_rt_chase_lev_steal(q: *mut ()) -> i64;
    pub fn datara_rt_chase_lev_size(q: *mut ()) -> i64;

    pub fn datara_rt_fast_memcpy(dest: *mut (), src: *const (), n: usize) -> *mut ();
    pub fn datara_rt_fast_memset(dest: *mut (), c: i32, n: usize) -> *mut ();
    pub fn datara_rt_fast_strncmp(s1: *const c_char, s2: *const c_char, n: usize) -> i32;
    pub fn datara_rt_fast_memcmp(s1: *const (), s2: *const (), n: usize) -> i32;

    pub fn datara_rt_pin_thread(core_id: i64) -> i32;
    pub fn datara_rt_get_current_core() -> i64;
    pub fn datara_rt_pin_worker_threads();

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
    pub fn datara_rt_byte_len(s: *const c_char) -> i64;
    pub fn datara_rt_str_chars(s: *const c_char) -> i64;
    pub fn datara_rt_char_len(s: *const c_char) -> i64;
    pub fn datara_rt_validate_utf8(s: *const c_char) -> i32;
    pub fn datara_rt_str_sanitize_utf8(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_next_scalar(s: *const c_char, inout_offset: *mut i64) -> *const c_char;
    pub fn datara_rt_str_scalar_at(s: *const c_char, offset: i64) -> *const c_char;
    pub fn datara_rt_str_byte_at(s: *const c_char, idx: i64) -> i64;
    pub fn datara_rt_str_next_offset(s: *const c_char, current_offset: i64) -> i64;
    pub fn datara_rt_str_contains(s: *const c_char, sub: *const c_char) -> i64;
    pub fn datara_rt_str_starts_with(s: *const c_char, pre: *const c_char) -> i64;
    pub fn datara_rt_str_ends_with(s: *const c_char, suf: *const c_char) -> i64;
    pub fn datara_rt_str_index_of(s: *const c_char, sub: *const c_char) -> i64;
    pub fn datara_rt_str_trim(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_to_int(s: *const c_char) -> i64;
    pub fn datara_rt_str_to_float(s: *const c_char) -> f64;
    pub fn datara_rt_str_substring(s: *const c_char, start: i64, len: i64) -> *const c_char;
    pub fn datara_rt_str_char_at(s: *const c_char, idx: i64) -> *const c_char;
    pub fn datara_rt_str_repeat(s: *const c_char, count: i64) -> *const c_char;
    pub fn datara_rt_str_pad_left(
        s: *const c_char,
        total_len: i64,
        pad: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_str_pad_right(
        s: *const c_char,
        total_len: i64,
        pad: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_str_replace(
        s: *const c_char,
        target: *const c_char,
        replacement: *const c_char,
    ) -> *const c_char;
    pub fn datara_rt_str_to_upper(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_to_lower(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_split(s: *const c_char, delim: *const c_char) -> *mut i64;
    pub fn datara_rt_str_join(list: *mut i64, delim: *const c_char) -> *const c_char;
    pub fn datara_rt_format_percent(val: f64, decimals: i64) -> *const c_char;
    pub fn datara_rt_format_int_with_commas(n: i64) -> *const c_char;

    pub fn datara_js_eval(code: *const c_char) -> *const c_char;
    pub fn datara_js_eval_int(code: *const c_char) -> i64;
    pub fn datara_js_eval_float(code: *const c_char) -> f64;
    pub fn datara_js_require(module_name: *const c_char) -> i64;
    pub fn datara_js_call(fn_name: *const c_char, args_json: *const c_char) -> *const c_char;
    pub fn datara_js_call_0(fn_name: *const c_char) -> *const c_char;
    pub fn datara_js_call_1(fn_name: *const c_char, arg0: *const c_char) -> *const c_char;
    pub fn datara_js_call_2(
        fn_name: *const c_char,
        arg0: *const c_char,
        arg1: *const c_char,
    ) -> *const c_char;
    pub fn datara_js_set_global(name: *const c_char, json_val: *const c_char) -> i64;
    pub fn datara_js_get_global(name: *const c_char) -> *const c_char;

    pub fn datara_py_eval(code: *const c_char) -> *const c_char;
    pub fn datara_py_eval_safe(code: *const c_char) -> *const c_char;
    pub fn datara_py_eval_int(code: *const c_char) -> i64;
    pub fn datara_py_eval_float(code: *const c_char) -> f64;
    pub fn datara_py_exec(code: *const c_char) -> i32;
    pub fn datara_py_import(module_name: *const c_char) -> u32;
    pub fn datara_py_call(fn_name: *const c_char, args_json: *const c_char) -> *const c_char;
    pub fn datara_py_call_1_str(fn_name: *const c_char, arg0: *const c_char) -> *const c_char;
    pub fn datara_py_call_1_float(fn_name: *const c_char, arg0: f64) -> f64;
    pub fn datara_py_last_error() -> *const c_char;
    pub fn datara_py_clear_error();
    pub fn datara_py_is_available() -> i32;
    pub fn datara_py_export_list_f64(var_name: *const c_char, list: *mut i64) -> i32;
    pub fn datara_py_assert_same_ptr(var_name: *const c_char, list: *mut i64) -> i32;

    pub fn datara_rt_file_read(path: *const c_char) -> *const c_char;
    pub fn datara_rt_file_write(path: *const c_char, content: *const c_char) -> i64;
    pub fn datara_rt_file_append(path: *const c_char, content: *const c_char) -> i64;
    pub fn datara_rt_file_exists(path: *const c_char) -> i64;

    pub fn datara_rt_sleep(ms: i64);
    pub fn datara_rt_now_ms() -> i64;
    pub fn datara_rt_now_unix_ms() -> i64;
    pub fn datara_rt_now_precise_ms() -> i64;
    pub fn datara_rt_now_ns() -> i64;
    pub fn now_ms() -> i64;
    pub fn datara_rt_env_get(key: *const c_char) -> *const c_char;
    pub fn datara_rt_path_join(a: *const c_char, b: *const c_char) -> *const c_char;
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
    pub fn datara_rt_map_create_1(k1: i64, v1: i64) -> *mut ();
    pub fn datara_rt_map_create_3(k1: i64, v1: i64, k2: i64, v2: i64, k3: i64, v3: i64) -> *mut ();
    pub fn datara_rt_map_create_4(
        k1: i64,
        v1: i64,
        k2: i64,
        v2: i64,
        k3: i64,
        v3: i64,
        k4: i64,
        v4: i64,
    ) -> *mut ();
    pub fn datara_rt_map_create_5(
        k1: i64,
        v1: i64,
        k2: i64,
        v2: i64,
        k3: i64,
        v3: i64,
        k4: i64,
        v4: i64,
        k5: i64,
        v5: i64,
    ) -> *mut ();
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
    pub fn datara_rt_http_get(url: *const c_char) -> *const c_char;

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
    pub fn datara_rt_math_clamp(val: f64, min_val: f64, max_val: f64) -> f64;
    pub fn datara_rt_math_hypot(a: f64, b: f64) -> f64;
    pub fn datara_rt_math_log(x: f64) -> f64;
    pub fn datara_rt_math_exp(x: f64) -> f64;
    pub fn datara_rt_math_min_int(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_max_int(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_clamp_int(val: i64, min_val: i64, max_val: i64) -> i64;
    pub fn datara_rt_math_abs_int(x: i64) -> i64;
    pub fn datara_rt_math_ctz(x: i64) -> i64;
    pub fn datara_rt_math_shr(v: i64, s: i64) -> i64;
    pub fn datara_rt_math_shl(v: i64, s: i64) -> i64;
    pub fn datara_rt_math_xor(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_and(a: i64, b: i64) -> i64;
    pub fn datara_rt_math_or(a: i64, b: i64) -> i64;

    pub fn datara_rt_slice(list: *mut i64, start: i64, end: i64) -> *mut i64;
    pub fn datara_rt_out_val(v: i64);
    pub fn datara_rt_parallel_for(start: i64, end: i64, worker_fn: usize, ctx: usize);
    pub fn datara_rt_parallel_invoke(fn1: usize, ctx1: usize, fn2: usize, ctx2: usize);
    pub fn datara_rt_num_workers() -> i64;

    pub fn datara_rt_arena_alloc(bytes: i64) -> *mut ();
    pub fn datara_rt_arena_checkpoint() -> i64;
    pub fn datara_rt_arena_reset(saved_top: i64);
    pub fn datara_rt_free(ptr: *mut ());
    pub fn datara_rt_str_free(s: *const c_char);
    pub fn datara_rt_list_free(list: *mut ());
    pub fn datara_rt_own_acquire(val: i64) -> i64;
    pub fn datara_rt_own_release(val: i64);

    pub fn datara_rt_pool_alloc(sz: usize) -> *mut ();
    pub fn datara_rt_pool_free(ptr: *mut (), sz: usize);
    pub fn datara_rt_box_alloc(val: i64) -> *mut i64;
    pub fn datara_rt_box_get(b: *mut i64) -> i64;
    pub fn datara_rt_box_free(b: *mut i64);
    pub fn datara_rt_str_sso(s: *const c_char) -> *const c_char;
    pub fn datara_rt_str_is_sso(s: *const c_char) -> i64;
    pub fn datara_rt_heap_alloc_count() -> i64;
    pub fn datara_rt_reset_heap_alloc_count();
    pub fn datara_rt_list_init_stack(stack_buf: *mut (), cap: i64) -> *mut i64;
    pub fn datara_rt_list_is_small_vec(list: *mut i64) -> i64;

    pub fn datara_rt_overflow_panic();
    pub fn datara_rt_div_zero_panic();
    pub fn datara_rt_checked_add(a: i64, b: i64) -> i64;
    pub fn datara_rt_checked_sub(a: i64, b: i64) -> i64;
    pub fn datara_rt_checked_mul(a: i64, b: i64) -> i64;
    pub fn datara_rt_checked_div(a: i64, b: i64) -> i64;
    pub fn datara_rt_checked_rem(a: i64, b: i64) -> i64;
    pub fn datara_rt_saturating_add(a: i64, b: i64) -> i64;
    pub fn datara_rt_saturating_sub(a: i64, b: i64) -> i64;
    pub fn datara_rt_saturating_mul(a: i64, b: i64) -> i64;
    pub fn datara_rt_wrapping_add(a: i64, b: i64) -> i64;
    pub fn datara_rt_wrapping_sub(a: i64, b: i64) -> i64;
    pub fn datara_rt_wrapping_mul(a: i64, b: i64) -> i64;

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

    reg!("datara_rt_overflow_panic", datara_rt_overflow_panic);
    reg!("datara_rt_div_zero_panic", datara_rt_div_zero_panic);
    reg!("datara_rt_checked_add", datara_rt_checked_add);
    reg!("datara_rt_checked_sub", datara_rt_checked_sub);
    reg!("datara_rt_checked_mul", datara_rt_checked_mul);
    reg!("datara_rt_checked_div", datara_rt_checked_div);
    reg!("datara_rt_checked_rem", datara_rt_checked_rem);
    reg!("datara_rt_saturating_add", datara_rt_saturating_add);
    reg!("datara_rt_saturating_sub", datara_rt_saturating_sub);
    reg!("datara_rt_saturating_mul", datara_rt_saturating_mul);
    reg!("datara_rt_wrapping_add", datara_rt_wrapping_add);
    reg!("datara_rt_wrapping_sub", datara_rt_wrapping_sub);
    reg!("datara_rt_wrapping_mul", datara_rt_wrapping_mul);
    reg!("wrapping_add", datara_rt_wrapping_add);
    reg!("wrapping_sub", datara_rt_wrapping_sub);
    reg!("wrapping_mul", datara_rt_wrapping_mul);
    reg!("saturating_add", datara_rt_saturating_add);
    reg!("saturating_sub", datara_rt_saturating_sub);
    reg!("saturating_mul", datara_rt_saturating_mul);

    reg!("datara_rt_out_int", datara_rt_out_int);
    reg!("datara_rt_out_bool", datara_rt_out_bool);
    reg!("datara_rt_bool_to_str", datara_rt_bool_to_str);
    reg!("datara_rt_out_float", datara_rt_out_float);
    reg!("datara_rt_float_to_str", datara_rt_float_to_str);
    reg!("float_to_str", datara_rt_float_to_str);
    reg!("datara_rt_out_str", datara_rt_out_str);
    reg!("datara_rt_out_dec64", datara_rt_out_dec64);
    reg!("datara_rt_err", datara_rt_err);
    reg!("datara_rt_exit", datara_rt_exit);
    reg!("datara_rt_pgo_hit_func", datara_rt_pgo_hit_func);
    reg!("datara_rt_pgo_hit_branch", datara_rt_pgo_hit_branch);
    reg!("datara_rt_pgo_hit_loop", datara_rt_pgo_hit_loop);
    reg!(
        "datara_rt_pgo_set_output_file",
        datara_rt_pgo_set_output_file
    );
    reg!("datara_rt_pgo_flush", datara_rt_pgo_flush);
    reg!("datara_rt_pgo_reset", datara_rt_pgo_reset);
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
    reg!("datara_rt_print_backtrace", datara_rt_print_backtrace);
    reg!("print_backtrace", datara_rt_print_backtrace);
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
    reg!("datara_rt_byte_len", datara_rt_byte_len);
    reg!("byte_len", datara_rt_byte_len);
    reg!("datara_rt_str_chars", datara_rt_str_chars);
    reg!("str_chars", datara_rt_str_chars);
    reg!("datara_rt_char_len", datara_rt_char_len);
    reg!("char_len", datara_rt_char_len);
    reg!("datara_rt_validate_utf8", datara_rt_validate_utf8);
    reg!("validate_utf8", datara_rt_validate_utf8);
    reg!("datara_rt_str_sanitize_utf8", datara_rt_str_sanitize_utf8);
    reg!("str_sanitize_utf8", datara_rt_str_sanitize_utf8);
    reg!("datara_rt_str_next_scalar", datara_rt_str_next_scalar);
    reg!("datara_rt_str_scalar_at", datara_rt_str_scalar_at);
    reg!("str_scalar_at", datara_rt_str_scalar_at);
    reg!("datara_rt_str_byte_at", datara_rt_str_byte_at);
    reg!("str_byte_at", datara_rt_str_byte_at);
    reg!("byte_at", datara_rt_str_byte_at);
    reg!("datara_rt_str_next_offset", datara_rt_str_next_offset);
    reg!("str_next_offset", datara_rt_str_next_offset);
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
    reg!("datara_rt_str_repeat", datara_rt_str_repeat);
    reg!("str_repeat", datara_rt_str_repeat);
    reg!("datara_rt_str_pad_left", datara_rt_str_pad_left);
    reg!("str_pad_left", datara_rt_str_pad_left);
    reg!("datara_rt_str_pad_right", datara_rt_str_pad_right);
    reg!("str_pad_right", datara_rt_str_pad_right);
    reg!("datara_rt_str_replace", datara_rt_str_replace);
    reg!("str_replace", datara_rt_str_replace);
    reg!("datara_rt_str_to_upper", datara_rt_str_to_upper);
    reg!("str_to_upper", datara_rt_str_to_upper);
    reg!("datara_rt_str_to_lower", datara_rt_str_to_lower);
    reg!("str_to_lower", datara_rt_str_to_lower);
    reg!("datara_rt_str_split", datara_rt_str_split);
    reg!("str_split", datara_rt_str_split);
    reg!("split", datara_rt_str_split);
    reg!("datara_rt_str_join", datara_rt_str_join);
    reg!("str_join", datara_rt_str_join);
    reg!("join", datara_rt_str_join);
    reg!("datara_rt_format_percent", datara_rt_format_percent);
    reg!("format_percent", datara_rt_format_percent);
    reg!(
        "datara_rt_format_int_with_commas",
        datara_rt_format_int_with_commas
    );
    reg!("format_int_with_commas", datara_rt_format_int_with_commas);

    reg!("datara_js_eval", datara_js_eval);
    reg!("js_eval", datara_js_eval);
    reg!("datara_js_eval_int", datara_js_eval_int);
    reg!("js_eval_int", datara_js_eval_int);
    reg!("datara_js_eval_float", datara_js_eval_float);
    reg!("js_eval_float", datara_js_eval_float);
    reg!("datara_js_require", datara_js_require);
    reg!("js_require", datara_js_require);
    reg!("datara_js_call", datara_js_call);
    reg!("js_call", datara_js_call);
    reg!("datara_js_call_0", datara_js_call_0);
    reg!("js_call_0", datara_js_call_0);
    reg!("datara_js_call_1", datara_js_call_1);
    reg!("js_call_1", datara_js_call_1);
    reg!("datara_js_call_2", datara_js_call_2);
    reg!("js_call_2", datara_js_call_2);
    reg!("datara_js_set_global", datara_js_set_global);
    reg!("js_set_global", datara_js_set_global);
    reg!("datara_js_get_global", datara_js_get_global);
    reg!("js_get_global", datara_js_get_global);

    reg!("datara_py_eval", datara_py_eval);
    reg!("py_eval", datara_py_eval);
    reg!("datara_py_eval_safe", datara_py_eval_safe);
    reg!("py_eval_safe", datara_py_eval_safe);
    reg!("datara_py_eval_int", datara_py_eval_int);
    reg!("py_eval_int", datara_py_eval_int);
    reg!("datara_py_eval_float", datara_py_eval_float);
    reg!("py_eval_float", datara_py_eval_float);
    reg!("datara_py_exec", datara_py_exec);
    reg!("py_exec", datara_py_exec);
    reg!("datara_py_import", datara_py_import);
    reg!("py_import", datara_py_import);
    reg!("datara_py_call", datara_py_call);
    reg!("py_call", datara_py_call);
    reg!("datara_py_call_1_str", datara_py_call_1_str);
    reg!("py_call_1_str", datara_py_call_1_str);
    reg!("datara_py_call_1_float", datara_py_call_1_float);
    reg!("py_call_1_float", datara_py_call_1_float);
    reg!("datara_py_last_error", datara_py_last_error);
    reg!("py_last_error", datara_py_last_error);
    reg!("datara_py_clear_error", datara_py_clear_error);
    reg!("py_clear_error", datara_py_clear_error);
    reg!("datara_py_is_available", datara_py_is_available);
    reg!("py_is_available", datara_py_is_available);
    reg!("datara_py_export_list_f64", datara_py_export_list_f64);
    reg!("datara_py_assert_same_ptr", datara_py_assert_same_ptr);

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
    reg!("datara_rt_time_precise_ms", datara_rt_time_precise_ms);
    reg!("time_precise_ms", datara_rt_time_precise_ms);
    reg!("datara_rt_time_delta_ms", datara_rt_time_delta_ms);
    reg!("time_delta_ms", datara_rt_time_delta_ms);
    reg!("datara_rt_now_ns", datara_rt_now_ns);
    reg!("now_ns", datara_rt_now_ns);
    reg!("datara_rt_env_get", datara_rt_env_get);
    reg!("env_get", datara_rt_env_get);
    reg!("datara_rt_path_join", datara_rt_path_join);
    reg!("path_join", datara_rt_path_join);
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
    reg!("datara_rt_map_create_1", datara_rt_map_create_1);
    reg!("datara_rt_map_create_3", datara_rt_map_create_3);
    reg!("datara_rt_map_create_4", datara_rt_map_create_4);
    reg!("datara_rt_map_create_5", datara_rt_map_create_5);
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
    reg!("datara_rt_math_clamp", datara_rt_math_clamp);
    reg!("math_clamp", datara_rt_math_clamp);
    reg!("datara_rt_math_hypot", datara_rt_math_hypot);
    reg!("math_hypot", datara_rt_math_hypot);
    reg!("datara_rt_math_log", datara_rt_math_log);
    reg!("math_log", datara_rt_math_log);
    reg!("datara_rt_math_exp", datara_rt_math_exp);
    reg!("math_exp", datara_rt_math_exp);
    reg!("datara_rt_math_min_int", datara_rt_math_min_int);
    reg!("math_min_int", datara_rt_math_min_int);
    reg!("datara_rt_math_max_int", datara_rt_math_max_int);
    reg!("math_max_int", datara_rt_math_max_int);
    reg!("datara_rt_math_clamp_int", datara_rt_math_clamp_int);
    reg!("math_clamp_int", datara_rt_math_clamp_int);
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

    reg!("datara_rt_slice", datara_rt_slice);
    reg!("datara_rt_out_val", datara_rt_out_val);
    reg!("datara_rt_parallel_for", datara_rt_parallel_for);
    reg!("parallel_for", datara_rt_parallel_for);
    reg!("datara_rt_parallel_invoke", datara_rt_parallel_invoke);
    reg!("parallel_invoke", datara_rt_parallel_invoke);
    reg!("datara_rt_num_workers", datara_rt_num_workers);
    reg!("num_workers", datara_rt_num_workers);
    reg!("datara_rt_schedule_run", datara_rt_schedule_run);
    reg!("schedule_run", datara_rt_schedule_run);
    reg!("datara_rt_schedule_cancel", datara_rt_schedule_cancel);
    reg!("schedule_cancel", datara_rt_schedule_cancel);

    reg!("datara_rt_arena_alloc", datara_rt_arena_alloc);
    reg!("datara_rt_arena_checkpoint", datara_rt_arena_checkpoint);
    reg!("datara_rt_arena_reset", datara_rt_arena_reset);
    reg!("datara_rt_free", datara_rt_free);
    reg!("datara_rt_str_free", datara_rt_str_free);
    reg!("datara_rt_list_free", datara_rt_list_free);
    reg!("datara_rt_own_acquire", datara_rt_own_acquire);
    reg!("datara_rt_own_release", datara_rt_own_release);

    reg!("datara_rt_pool_alloc", datara_rt_pool_alloc);
    reg!("datara_rt_pool_free", datara_rt_pool_free);
    reg!("datara_rt_box_alloc", datara_rt_box_alloc);
    reg!("datara_rt_box_get", datara_rt_box_get);
    reg!("datara_rt_box_free", datara_rt_box_free);
    reg!("datara_rt_str_sso", datara_rt_str_sso);
    reg!("datara_rt_str_is_sso", datara_rt_str_is_sso);
    reg!("datara_rt_heap_alloc_count", datara_rt_heap_alloc_count);
    reg!(
        "datara_rt_reset_heap_alloc_count",
        datara_rt_reset_heap_alloc_count
    );
    reg!("datara_rt_list_init_stack", datara_rt_list_init_stack);
    reg!("datara_rt_list_is_small_vec", datara_rt_list_is_small_vec);

    reg!("datara_rt_chase_lev_create", datara_rt_chase_lev_create);
    reg!("datara_rt_chase_lev_destroy", datara_rt_chase_lev_destroy);
    reg!("datara_rt_chase_lev_push", datara_rt_chase_lev_push);
    reg!("datara_rt_chase_lev_pop", datara_rt_chase_lev_pop);
    reg!("datara_rt_chase_lev_steal", datara_rt_chase_lev_steal);
    reg!("datara_rt_chase_lev_size", datara_rt_chase_lev_size);

    reg!("datara_rt_fast_memcpy", datara_rt_fast_memcpy);
    reg!("datara_rt_fast_memset", datara_rt_fast_memset);
    reg!("datara_rt_fast_strncmp", datara_rt_fast_strncmp);
    reg!("datara_rt_fast_memcmp", datara_rt_fast_memcmp);

    reg!("datara_rt_pin_thread", datara_rt_pin_thread);
    reg!("datara_rt_get_current_core", datara_rt_get_current_core);
    reg!("datara_rt_pin_worker_threads", datara_rt_pin_worker_threads);
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
        // An argument containing an interior NUL cannot round-trip through a
        // C argv; drop it with a warning instead of silently passing "".
        match CString::new(a.as_str()) {
            Ok(c) => c_strings.push(c),
            Err(_) => {
                eprintln!("warning: dropping command-line argument containing NUL byte");
            }
        }
    }
    let mut c_ptrs: Vec<*const c_char> = c_strings.iter().map(|s| s.as_ptr()).collect();
    let argc = c_ptrs.len() as i32;
    c_ptrs.push(std::ptr::null());
    let argv = c_ptrs.as_ptr();

    unsafe {
        datara_rt_set_args(argc, argv);
    }

    if capture {
        unsafe {
            datara_rt_clear_capture();
            datara_rt_set_capture(1);
        }
    }

    let start = Instant::now();
    let exit_code = unsafe { main_fn(argc, argv) };
    let duration = start.elapsed().as_millis();

    unsafe {
        datara_rt_set_args(0, std::ptr::null());
    }

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
