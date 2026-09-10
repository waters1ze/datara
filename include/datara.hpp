#ifndef DATARA_HPP
#define DATARA_HPP

#include "datara.h"
#include <string>
#include <string_view>
#include <vector>
#include <stdexcept>
#include <utility>
#include <optional>

namespace datara {

/**
 * Zero-copy string view wrapping datara_string_t.
 */
class StringView {
public:
    constexpr StringView() noexcept : view_{"", 0} {}
    constexpr StringView(const char* data, size_t size) noexcept : view_{data, size} {}
    constexpr StringView(std::string_view sv) noexcept : view_{sv.data(), sv.size()} {}
    constexpr StringView(datara_string_t raw) noexcept : view_(raw) {}

    constexpr const char* data() const noexcept { return view_.ptr; }
    constexpr size_t size() const noexcept { return view_.len; }
    constexpr bool empty() const noexcept { return view_.len == 0; }

    constexpr operator std::string_view() const noexcept {
        return std::string_view(view_.ptr, view_.len);
    }

    std::string to_string() const {
        return std::string(view_.ptr, view_.len);
    }

    constexpr datara_string_t raw() const noexcept { return view_; }

private:
    datara_string_t view_;
};

/**
 * Zero-copy contiguous buffer span wrapping datara_slice_t.
 */
template <typename T>
class Span {
public:
    using element_type = T;
    using value_type = std::remove_cv_t<T>;
    using size_type = size_t;
    using pointer = T*;
    using const_pointer = const T*;

    constexpr Span() noexcept : data_(nullptr), size_(0) {}
    constexpr Span(pointer data, size_type size) noexcept : data_(data), size_(size) {}
    constexpr Span(datara_slice_t slice) noexcept
        : data_(reinterpret_cast<pointer>(slice.data)), size_(slice.len) {}

    constexpr pointer data() const noexcept { return data_; }
    constexpr size_type size() const noexcept { return size_; }
    constexpr bool empty() const noexcept { return size_ == 0; }

    constexpr T& operator[](size_type idx) const { return data_[idx]; }

    constexpr pointer begin() const noexcept { return data_; }
    constexpr pointer end() const noexcept { return data_ + size_; }

    constexpr datara_slice_t raw_slice() const noexcept {
        return datara_slice_make(data_, size_);
    }

private:
    pointer data_;
    size_type size_;
};

/**
 * Type-safe outcome / result type wrapping datara_outcome_t.
 */
template <typename T, typename E = std::string>
class Outcome {
public:
    static Outcome ok(T value) {
        Outcome o;
        o.is_ok_ = true;
        o.val_ = std::move(value);
        return o;
    }

    static Outcome err(E error) {
        Outcome o;
        o.is_ok_ = false;
        o.err_ = std::move(error);
        return o;
    }

    bool is_ok() const noexcept { return is_ok_; }
    bool is_err() const noexcept { return !is_ok_; }

    const T& unwrap() const {
        if (!is_ok_) {
            throw std::runtime_error("Called unwrap on an error Outcome: " + std::string(err_));
        }
        return val_;
    }

    T unwrap_or(T default_val) const {
        return is_ok_ ? val_ : default_val;
    }

    const E& error() const {
        return err_;
    }

private:
    bool is_ok_{false};
    T val_{};
    E err_{};
};

/**
 * RAII Module Wrapper for Datara compiler and runtime.
 * Implements strict zero-cost move semantics and unique ownership.
 */
class Module {
public:
    Module() = default;

    explicit Module(std::string_view path) {
        load(path);
    }

    ~Module() noexcept {
        reset();
    }

    // Move-only semantics (non-copyable)
    Module(const Module&) = delete;
    Module& operator=(const Module&) = delete;

    Module(Module&& other) noexcept
        : path_(std::move(other.path_)), is_loaded_(other.is_loaded_) {
        other.is_loaded_ = false;
    }

    Module& operator=(Module&& other) noexcept {
        if (this != &other) {
            reset();
            path_ = std::move(other.path_);
            is_loaded_ = other.is_loaded_;
            other.is_loaded_ = false;
        }
        return *this;
    }

    void load(std::string_view path) {
        reset();
        int32_t rc = forgen_init();
        if (rc != 0) {
            throw std::runtime_error(std::string("Failed to initialize Datara runtime: ") + forgen_last_error());
        }

        std::string null_terminated_path(path);
        rc = forgen_load_module(null_terminated_path.c_str());
        if (rc != 0) {
            throw std::runtime_error(std::string("Failed to load module: ") + forgen_last_error());
        }
        path_ = std::move(null_terminated_path);
        is_loaded_ = true;
    }

    bool is_loaded() const noexcept { return is_loaded_; }

    template <typename Ret = int64_t, typename... Args>
    Ret call(const char* func_name, Args... args) {
        if (!is_loaded_) {
            throw std::runtime_error("Attempted to call function on unloaded Datara module");
        }
        int64_t raw_args[] = { static_cast<int64_t>(args)..., 0 };
        size_t count = sizeof...(Args);
        int64_t out_res = 0;
        int32_t rc = forgen_call_fn(func_name, count > 0 ? raw_args : nullptr, count, &out_res);
        if (rc != 0) {
            throw std::runtime_error(std::string("Function call failed: ") + forgen_last_error());
        }
        return static_cast<Ret>(out_res);
    }

    void reset() noexcept {
        if (is_loaded_) {
            forgen_shutdown();
            is_loaded_ = false;
            path_.clear();
        }
    }

private:
    std::string path_;
    bool is_loaded_{false};
};

} // namespace datara

#endif /* DATARA_HPP */
