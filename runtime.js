/* @ts-self-types="./runtime.d.ts" */
import { registerGuestMemory, releaseGuestMemory } from './guest-memory.js';
import { updateU32 } from './memory-ops.js';
import * as import1 from "./guest-memory.js"


/**
 * A completed RGBA frame copied out once by the browser presentation host.
 */
export class Frame {
    static __wrap(ptr) {
        const obj = Object.create(Frame.prototype);
        obj.__wbg_ptr = ptr;
        FrameFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FrameFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_frame_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get height() {
        const ret = wasm.frame_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {Uint8Array}
     */
    pixels() {
        const ret = wasm.frame_pixels(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get width() {
        const ret = wasm.frame_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Frame.prototype[Symbol.dispose] = Frame.prototype.free;

/**
 * Retains the exact completed surface until its packed bytes are copied to the host.
 */
export class PackedFrame {
    static __wrap(ptr) {
        const obj = Object.create(PackedFrame.prototype);
        obj.__wbg_ptr = ptr;
        PackedFrameFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PackedFrameFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_packedframe_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get format() {
        const ret = wasm.packedframe_format(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get height() {
        const ret = wasm.packedframe_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get pitch() {
        const ret = wasm.packedframe_pitch(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {Uint8Array}
     */
    pixels() {
        const ret = wasm.packedframe_pixels(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get width() {
        const ret = wasm.packedframe_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) PackedFrame.prototype[Symbol.dispose] = PackedFrame.prototype.free;

/**
 * One explicit process per guest memory. free() drops metadata, not the caller's memory.
 */
export class Process {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProcessFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_process_free(ptr, 0);
    }
    /**
     * @param {string} path
     * @param {number} size
     * @param {Function} reader
     */
    addFileReader(path, size, reader) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_addFileReader(this.__wbg_ptr, ptr0, len0, size, reader);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} path
     * @param {Uint8Array} bytes
     */
    addFile(path, bytes) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.process_addFile(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} address
     * @param {number} size
     * @param {boolean} writable
     */
    adopt(address, size, writable) {
        const ret = wasm.process_adopt(this.__wbg_ptr, address, size, writable);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} now
     */
    advanceTime(now) {
        const ret = wasm.process_advanceTime(this.__wbg_ptr, now);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} id
     * @param {Uint32Array} args
     * @returns {string}
     */
    beginCall(id, args) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passArray32ToWasm0(args, wasm.__wbindgen_malloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.process_beginCall(this.__wbg_ptr, id, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {number} id
     * @param {Uint32Array} args
     * @param {Uint32Array} registers
     * @returns {string}
     */
    beginException(id, args, registers) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passArray32ToWasm0(args, wasm.__wbindgen_malloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray32ToWasm0(registers, wasm.__wbindgen_malloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.process_beginException(this.__wbg_ptr, id, ptr0, len0, ptr1, len1);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * A DWORD fits exactly in f64, leaving -1 for transport failure. Returning
     * one scalar avoids allocating a JS array for the Result multi-value ABI.
     * The synchronous caller consumes an error before another guest can run.
     * @param {number} thread_id
     * @param {number} id
     * @param {number} count
     * @param {number} first
     * @param {number} second
     * @param {number} third
     * @param {number} fourth
     * @param {number | null} [now_ms]
     * @param {number | null} [wall_ms]
     * @returns {number}
     */
    callSmallForThreadStatus(thread_id, id, count, first, second, third, fourth, now_ms, wall_ms) {
        const ret = wasm.process_callSmallForThreadStatus(this.__wbg_ptr, thread_id, id, count, first, second, third, fourth, !isLikeNone(now_ms), isLikeNone(now_ms) ? 0 : now_ms, !isLikeNone(wall_ms), isLikeNone(wall_ms) ? 0 : wall_ms);
        return ret;
    }
    /**
     * Select the calling thread and invoke the scalar API in one boundary crossing.
     * Selection precedes validation, just as `setThread` followed by `callSmall` does.
     * @param {number} thread_id
     * @param {number} id
     * @param {number} count
     * @param {number} first
     * @param {number} second
     * @param {number} third
     * @param {number} fourth
     * @param {number | null} [now_ms]
     * @param {number | null} [wall_ms]
     * @returns {number}
     */
    callSmallForThread(thread_id, id, count, first, second, third, fourth, now_ms, wall_ms) {
        const ret = wasm.process_callSmallForThread(this.__wbg_ptr, thread_id, id, count, first, second, third, fourth, !isLikeNone(now_ms), isLikeNone(now_ms) ? 0 : now_ms, !isLikeNone(wall_ms), isLikeNone(wall_ms) ? 0 : wall_ms);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Common synchronous calls cross the JS/Wasm boundary with scalar
     * arguments, avoiding a temporary allocation for wasm-bindgen's slice.
     * Keep the same arity, clock, memory and API validation as `call`.
     * @param {number} id
     * @param {number} count
     * @param {number} first
     * @param {number} second
     * @param {number} third
     * @param {number} fourth
     * @param {number | null} [now_ms]
     * @param {number | null} [wall_ms]
     * @returns {number}
     */
    callSmall(id, count, first, second, third, fourth, now_ms, wall_ms) {
        const ret = wasm.process_callSmall(this.__wbg_ptr, id, count, first, second, third, fourth, !isLikeNone(now_ms), isLikeNone(now_ms) ? 0 : now_ms, !isLikeNone(wall_ms), isLikeNone(wall_ms) ? 0 : wall_ms);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} id
     * @param {Uint32Array} args
     * @param {number | null} [now_ms]
     * @param {number | null} [wall_ms]
     * @returns {number}
     */
    call(id, args, now_ms, wall_ms) {
        const ptr0 = passArray32ToWasm0(args, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_call(this.__wbg_ptr, id, ptr0, len0, !isLikeNone(now_ms), isLikeNone(now_ms) ? 0 : now_ms, !isLikeNone(wall_ms), isLikeNone(wall_ms) ? 0 : wall_ms);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} token
     * @returns {boolean}
     */
    cancelCall(token) {
        const ret = wasm.process_cancelCall(this.__wbg_ptr, token);
        return ret !== 0;
    }
    /**
     * @param {number} token
     * @returns {boolean}
     */
    cancelException(token) {
        const ret = wasm.process_cancelException(this.__wbg_ptr, token);
        return ret !== 0;
    }
    /**
     * @param {number} token
     * @returns {boolean}
     */
    cancelWait(token) {
        const ret = wasm.process_cancelWait(this.__wbg_ptr, token);
        return ret !== 0;
    }
    /**
     * @param {number} id
     * @param {Int32Array} registers
     * @param {number} monotonic_ms
     * @returns {Int32Array}
     */
    cpuHelper(id, registers, monotonic_ms) {
        const ptr0 = passArray32ToWasm0(registers, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_cpuHelper(this.__wbg_ptr, id, ptr0, len0, monotonic_ms);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayI32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
    /**
     * @param {string} dll
     * @param {any} symbol
     * @returns {number}
     */
    exportAddress(dll, symbol) {
        const ptr0 = passStringToWasm0(dll, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_exportAddress(this.__wbg_ptr, ptr0, len0, symbol);
        return ret >>> 0;
    }
    /**
     * @param {number} id
     * @param {number} code
     */
    finishThread(id, code) {
        const ret = wasm.process_finishThread(this.__wbg_ptr, id, code);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {boolean}
     */
    hasReadyWaits() {
        const ret = wasm.process_hasReadyWaits(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @param {number} size
     * @returns {number}
     */
    heapAlloc(size) {
        const ret = wasm.process_heapAlloc(this.__wbg_ptr, size);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} address
     */
    heapFree(address) {
        const ret = wasm.process_heapFree(this.__wbg_ptr, address);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} address
     * @param {number} size
     * @returns {number}
     */
    heapRealloc(address, size) {
        const ret = wasm.process_heapRealloc(this.__wbg_ptr, address, size);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} address
     * @returns {number}
     */
    heapSize(address) {
        const ret = wasm.process_heapSize(this.__wbg_ptr, address);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {string} json
     */
    importRegistry(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_importRegistry(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {string}
     */
    imports() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.process_imports(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {string} configuration
     */
    installModules(configuration) {
        const ptr0 = passStringToWasm0(configuration, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_installModules(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} key
     * @param {boolean} down
     * @param {boolean} repeat
     */
    keyInput(key, down, repeat) {
        const ret = wasm.process_keyInput(this.__wbg_ptr, key, down, repeat);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} address
     * @param {Uint8Array} bytes
     * @param {boolean} writable
     */
    map(address, bytes, writable) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_map(this.__wbg_ptr, address, ptr0, len0, writable);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} frames
     * @param {number} rate
     * @param {number} now_ms
     * @returns {StereoBlock}
     */
    mixAudio(frames, rate, now_ms) {
        const ret = wasm.process_mixAudio(this.__wbg_ptr, frames, rate, now_ms);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return StereoBlock.__wrap(ret[0]);
    }
    /**
     * @param {number} x
     * @param {number} y
     * @param {number} buttons
     */
    mouseInput(x, y, buttons) {
        const ret = wasm.process_mouseInput(this.__wbg_ptr, x, y, buttons);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {WebAssembly.Memory} memory
     * @param {number} allocation_start
     * @param {number} limit
     */
    constructor(memory, allocation_start, limit) {
        const ret = wasm.process_new(memory, allocation_start, limit);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        ProcessFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @param {number} now_ms
     * @returns {string | undefined}
     */
    nextMediaTimer(now_ms) {
        const ret = wasm.process_nextMediaTimer(this.__wbg_ptr, now_ms);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]);
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
    /**
     * @param {number} address
     * @param {number} size
     * @returns {Uint8Array}
     */
    read(address, size) {
        const ret = wasm.process_read(this.__wbg_ptr, address, size);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {number} address
     * @param {number} size
     */
    reserve(address, size) {
        const ret = wasm.process_reserve(this.__wbg_ptr, address, size);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Resolve a supported Windows import to a JSON service descriptor.
     * @param {string} dll
     * @param {string} name
     * @returns {string | undefined}
     */
    resolve(dll, name) {
        const ptr0 = passStringToWasm0(dll, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.process_resolve(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        let v3;
        if (ret[0] !== 0) {
            v3 = getStringFromWasm0(ret[0], ret[1]);
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v3;
    }
    /**
     * @param {Uint8Array} bytes
     * @returns {number}
     */
    restoreJournal(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_restoreJournal(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} token
     * @param {number} result
     * @returns {string}
     */
    resumeCall(token, result) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.process_resumeCall(this.__wbg_ptr, token, result);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * @param {number} token
     * @param {Uint32Array} registers
     * @returns {string}
     */
    resumeException(token, registers) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passArray32ToWasm0(registers, wasm.__wbindgen_malloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.process_resumeException(this.__wbg_ptr, token, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Preserve structured wait/exit outcomes, but return immediate values
     * directly so uncontended locks need neither a slice copy nor JSON.
     * @param {number} id
     * @param {number} count
     * @param {number} first
     * @param {number} second
     * @param {number} third
     * @param {number} fourth
     * @param {number} now
     * @returns {any}
     */
    scheduleCallSmall(id, count, first, second, third, fourth, now) {
        const ret = wasm.process_scheduleCallSmall(this.__wbg_ptr, id, count, first, second, third, fourth, now);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {number} id
     * @param {Uint32Array} args
     * @param {number} now
     * @returns {string}
     */
    scheduleCall(id, args, now) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passArray32ToWasm0(args, wasm.__wbindgen_malloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.process_scheduleCall(this.__wbg_ptr, id, ptr0, len0, now);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {number} address
     */
    setCommandLine(address) {
        const ret = wasm.process_setCommandLine(this.__wbg_ptr, address);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} id
     */
    setThread(id) {
        const ret = wasm.process_setThread(this.__wbg_ptr, id);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    takeFileChanges() {
        const ret = wasm.process_takeFileChanges(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Frame | undefined}
     */
    takeFrame() {
        const ret = wasm.process_takeFrame(this.__wbg_ptr);
        return ret === 0 ? undefined : Frame.__wrap(ret);
    }
    /**
     * @returns {PackedFrame | undefined}
     */
    takePackedFrame() {
        const ret = wasm.process_takePackedFrame(this.__wbg_ptr);
        return ret === 0 ? undefined : PackedFrame.__wrap(ret);
    }
    /**
     * @returns {string}
     */
    takeReadyWaits() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.process_takeReadyWaits(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string | undefined}
     */
    takeRegistryChanges() {
        const ret = wasm.process_takeRegistryChanges(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]);
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
    /**
     * @returns {string}
     */
    takeRunnableThreads() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.process_takeRunnableThreads(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {any}
     */
    takeSyncCallError() {
        const ret = wasm.process_takeSyncCallError(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {number} address
     * @param {number} size
     * @returns {number}
     */
    virtualAlloc(address, size) {
        const ret = wasm.process_virtualAlloc(this.__wbg_ptr, address, size);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} address
     */
    virtualFree(address) {
        const ret = wasm.process_virtualFree(this.__wbg_ptr, address);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} address
     * @param {Uint8Array} bytes
     */
    write(address, bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.process_write(this.__wbg_ptr, address, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) Process.prototype[Symbol.dispose] = Process.prototype.free;

/**
 * Owned PCM block: only stereo sample arrays cross the browser boundary.
 */
export class StereoBlock {
    static __wrap(ptr) {
        const obj = Object.create(StereoBlock.prototype);
        obj.__wbg_ptr = ptr;
        StereoBlockFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        StereoBlockFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_stereoblock_free(ptr, 0);
    }
    /**
     * @returns {Float32Array}
     */
    left() {
        const ret = wasm.stereoblock_left(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Float32Array}
     */
    right() {
        const ret = wasm.stereoblock_right(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) StereoBlock.prototype[Symbol.dispose] = StereoBlock.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_30c8987f7c2ed4e2: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_debug_string_4687d8d8c2017d52: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_function_1f9d30630b8b1d3d: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_jsval_eq_02babf21faa37971: function(arg0, arg1) {
            const ret = arg0 === arg1;
            return ret;
        },
        __wbg___wbindgen_memory_caa4a6165639c8b5: function() {
            const ret = wasm.memory;
            return ret;
        },
        __wbg___wbindgen_number_get_2e0e7dee9f701a71: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_0380ccaa2f57f0d9: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_buffer_7afc7cca4d0cf036: function(arg0) {
            const ret = arg0.buffer;
            return ret;
        },
        __wbg_byteLength_a92cc1d458cf2beb: function(arg0) {
            const ret = arg0.byteLength;
            return ret;
        },
        __wbg_call_1875a20c43a36133: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.call(arg1, arg2, arg3);
            return ret;
        }, arguments); },
        __wbg_call_187d372bd5fdd4aa: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_fill_775586ed37049ccf: function(arg0, arg1, arg2, arg3) {
            const ret = arg0.fill(arg1, arg2 >>> 0, arg3 >>> 0);
            return ret;
        },
        __wbg_getUint16_5b2c49578d71afaf: function(arg0, arg1, arg2) {
            const ret = arg0.getUint16(arg1 >>> 0, arg2 !== 0);
            return ret;
        },
        __wbg_getUint32_628b16e2b275dfe3: function(arg0, arg1, arg2) {
            const ret = arg0.getUint32(arg1 >>> 0, arg2 !== 0);
            return ret;
        },
        __wbg_getUint8_a81dd53fa9a92b3b: function(arg0, arg1) {
            const ret = arg0.getUint8(arg1 >>> 0);
            return ret;
        },
        __wbg_get_31af05bd4842a84f: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_instanceof_RangeError_62996e9dbf747e67: function(arg0) {
            let result;
            try {
                result = arg0 instanceof RangeError;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_828cef2aaacafc31: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_length_7f3c00c40364105e: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_new_6a67771a6e4ff1af: function(arg0, arg1, arg2) {
            const ret = new DataView(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_new_from_slice_9a868026ffa4208a: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_from_slice_ca6ad97db1f4779a: function(arg0, arg1) {
            const ret = new Float32Array(getArrayF32FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_with_byte_offset_and_length_2f5d7fc2a828b74d: function(arg0, arg1, arg2) {
            const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_prototypesetcall_bc27214492979395: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_registerGuestMemory_55f9e9d1fb6463a1: function() { return handleError(function (arg0, arg1) {
            const ret = registerGuestMemory(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_releaseGuestMemory_5e72e09055c226e0: function() { return handleError(function (arg0) {
            releaseGuestMemory(arg0 >>> 0);
        }, arguments); },
        __wbg_setUint16_48624a2872c91a1c: function(arg0, arg1, arg2, arg3) {
            arg0.setUint16(arg1 >>> 0, arg2, arg3 !== 0);
        },
        __wbg_setUint32_f56b241670a12a45: function(arg0, arg1, arg2, arg3) {
            arg0.setUint32(arg1 >>> 0, arg2 >>> 0, arg3 !== 0);
        },
        __wbg_setUint8_03df75c26031482f: function(arg0, arg1, arg2) {
            arg0.setUint8(arg1 >>> 0, arg2);
        },
        __wbg_set_34d08fd992d43d61: function(arg0, arg1, arg2) {
            arg0.set(arg1, arg2 >>> 0);
        },
        __wbg_set_575d3ddb70fe831d: function(arg0, arg1, arg2) {
            arg0.set(getArrayU8FromWasm0(arg1, arg2));
        },
        __wbg_updateU32_7d7ab42960a1427d: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            const ret = updateU32(arg0, arg1 >>> 0, arg2 >>> 0, arg3 >>> 0, arg4 >>> 0);
            return ret;
        }, arguments); },
        __wbindgen_generic_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_generic_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./runtime_bg.js": import0,
        "./guest-memory.js": import1,
    };
}

const FrameFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_frame_free(ptr, 1));
const PackedFrameFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_packedframe_free(ptr, 1));
const ProcessFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_process_free(ptr, 1));
const StereoBlockFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_stereoblock_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayI32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getInt32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

let cachedInt32ArrayMemory0 = null;
function getInt32ArrayMemory0() {
    if (cachedInt32ArrayMemory0 === null || cachedInt32ArrayMemory0.byteLength === 0) {
        cachedInt32ArrayMemory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachedInt32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedInt32ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('runtime_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
