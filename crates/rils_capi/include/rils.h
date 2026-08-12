#ifndef RILS_H
#define RILS_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define RILS_API __declspec(dllimport)
#else
#define RILS_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef uint64_t RilsHandle;

typedef struct RilsSlice {
    const uint8_t *data;
    size_t length;
} RilsSlice;

typedef struct RilsValue {
    uint32_t tag;
    uint32_t reserved;
    uint64_t low;
    uint64_t high;
} RilsValue;

typedef struct RilsHostFunction {
    uint64_t function_id;
    RilsSlice name;
    RilsSlice capability;
    const uint32_t *parameter_tags;
    size_t parameter_count;
    uint32_t return_tag;
    uint32_t reserved;
} RilsHostFunction;

typedef int32_t (*RilsHostDispatcher)(
    void *user_data,
    uint64_t function_id,
    const RilsValue *arguments,
    size_t argument_count,
    RilsValue *out_value,
    RilsSlice *out_error);

enum RilsStatus {
    RILS_STATUS_OK = 0,
    RILS_STATUS_INVALID_ARGUMENT = 1,
    RILS_STATUS_INVALID_HANDLE = 2,
    RILS_STATUS_COMPILE_ERROR = 3,
    RILS_STATUS_EXECUTION_ERROR = 4,
    RILS_STATUS_UNSUPPORTED_VALUE = 5,
    RILS_STATUS_BYTECODE_ERROR = 6,
    RILS_STATUS_PANIC = 255
};

enum RilsValueTag {
    RILS_VALUE_UNIT = 0,
    RILS_VALUE_BOOL = 1,
    RILS_VALUE_I8 = 2,
    RILS_VALUE_I16 = 3,
    RILS_VALUE_I32 = 4,
    RILS_VALUE_I64 = 5,
    RILS_VALUE_I128 = 6,
    RILS_VALUE_ISIZE = 7,
    RILS_VALUE_U8 = 8,
    RILS_VALUE_U16 = 9,
    RILS_VALUE_U32 = 10,
    RILS_VALUE_U64 = 11,
    RILS_VALUE_U128 = 12,
    RILS_VALUE_USIZE = 13,
    RILS_VALUE_F32 = 14,
    RILS_VALUE_F64 = 15,
    RILS_VALUE_CHAR = 16
};

RILS_API uint32_t rils_abi_version(void);
RILS_API RilsHandle rils_runtime_create(void);
RILS_API int32_t rils_runtime_destroy(RilsHandle runtime);
RILS_API int32_t rils_runtime_set_max_steps(RilsHandle runtime, uint64_t max_steps);
RILS_API int32_t rils_runtime_register_host_functions(
    RilsHandle runtime,
    const RilsHostFunction *functions,
    size_t function_count);
RILS_API int32_t rils_runtime_register_host_manifest(
    RilsHandle runtime,
    RilsSlice manifest);
RILS_API int32_t rils_runtime_host_manifest_size(RilsHandle runtime, size_t *out_size);
RILS_API int32_t rils_runtime_write_host_manifest(
    RilsHandle runtime,
    uint8_t *buffer,
    size_t buffer_capacity,
    size_t *out_written);
RILS_API int32_t rils_runtime_set_host_dispatcher(
    RilsHandle runtime,
    RilsHostDispatcher dispatcher,
    void *user_data);
RILS_API int32_t rils_runtime_allow_capability(RilsHandle runtime, RilsSlice capability);
RILS_API int32_t rils_runtime_freeze_host_registry(RilsHandle runtime);
RILS_API int32_t rils_module_compile(
    RilsHandle runtime,
    RilsSlice source_name,
    RilsSlice source,
    RilsHandle *out_module);
RILS_API int32_t rils_module_compile_file(
    RilsHandle runtime,
    RilsSlice path,
    RilsHandle *out_module);
RILS_API int32_t rils_module_load_bytecode(
    RilsHandle runtime,
    RilsSlice bytecode,
    RilsHandle *out_module);
RILS_API int32_t rils_module_load_bytecode_file(
    RilsHandle runtime,
    RilsSlice path,
    RilsHandle *out_module);
RILS_API int32_t rils_module_validate_host(RilsHandle runtime, RilsHandle module);
RILS_API int32_t rils_module_bytecode_size(
    RilsHandle runtime,
    RilsHandle module,
    size_t *out_size);
RILS_API int32_t rils_module_write_bytecode(
    RilsHandle runtime,
    RilsHandle module,
    uint8_t *buffer,
    size_t buffer_capacity,
    size_t *out_written);
RILS_API int32_t rils_module_write_bytecode_file(
    RilsHandle runtime,
    RilsHandle module,
    RilsSlice path);
RILS_API int32_t rils_module_destroy(RilsHandle runtime, RilsHandle module);
RILS_API int32_t rils_instance_create(
    RilsHandle runtime,
    RilsHandle module,
    RilsHandle *out_instance);
RILS_API int32_t rils_instance_destroy(RilsHandle runtime, RilsHandle instance);
RILS_API int32_t rils_instance_execute(
    RilsHandle runtime,
    RilsHandle instance,
    RilsValue *out_value);
RILS_API int32_t rils_instance_call(
    RilsHandle runtime,
    RilsHandle instance,
    RilsSlice function_name,
    const RilsValue *arguments,
    size_t argument_count,
    RilsValue *out_value);

RILS_API int32_t rils_last_error_code(void);
RILS_API RilsSlice rils_last_error_message(void);
RILS_API RilsSlice rils_last_error_source_name(void);
RILS_API uint64_t rils_last_error_span_start(void);
RILS_API uint64_t rils_last_error_span_end(void);

#ifdef __cplusplus
}
#endif

#endif
