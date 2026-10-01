//! The Java Native Interface as COBOL reaches it: the JNI environment structure, a pointer to the
//! function table whose slots the JNI specification numbers, the first four reserved.

/// The JNI functions in function-table order, from slot 4, through GetModule (Java 9).
pub const FUNCTIONS: &[&str] = &[
    "GetVersion", "DefineClass", "FindClass", "FromReflectedMethod", "FromReflectedField", "ToReflectedMethod", "GetSuperclass",
    "IsAssignableFrom", "ToReflectedField", "Throw", "ThrowNew", "ExceptionOccurred", "ExceptionDescribe", "ExceptionClear", "FatalError",
    "PushLocalFrame", "PopLocalFrame", "NewGlobalRef", "DeleteGlobalRef", "DeleteLocalRef", "IsSameObject", "NewLocalRef",
    "EnsureLocalCapacity", "AllocObject", "NewObject", "NewObjectV", "NewObjectA", "GetObjectClass", "IsInstanceOf", "GetMethodID",
    "CallObjectMethod", "CallObjectMethodV", "CallObjectMethodA", "CallBooleanMethod", "CallBooleanMethodV", "CallBooleanMethodA",
    "CallByteMethod", "CallByteMethodV", "CallByteMethodA", "CallCharMethod", "CallCharMethodV", "CallCharMethodA", "CallShortMethod",
    "CallShortMethodV", "CallShortMethodA", "CallIntMethod", "CallIntMethodV", "CallIntMethodA", "CallLongMethod", "CallLongMethodV",
    "CallLongMethodA", "CallFloatMethod", "CallFloatMethodV", "CallFloatMethodA", "CallDoubleMethod", "CallDoubleMethodV",
    "CallDoubleMethodA", "CallVoidMethod", "CallVoidMethodV", "CallVoidMethodA", "CallNonvirtualObjectMethod",
    "CallNonvirtualObjectMethodV", "CallNonvirtualObjectMethodA", "CallNonvirtualBooleanMethod", "CallNonvirtualBooleanMethodV",
    "CallNonvirtualBooleanMethodA", "CallNonvirtualByteMethod", "CallNonvirtualByteMethodV", "CallNonvirtualByteMethodA",
    "CallNonvirtualCharMethod", "CallNonvirtualCharMethodV", "CallNonvirtualCharMethodA", "CallNonvirtualShortMethod",
    "CallNonvirtualShortMethodV", "CallNonvirtualShortMethodA", "CallNonvirtualIntMethod", "CallNonvirtualIntMethodV",
    "CallNonvirtualIntMethodA", "CallNonvirtualLongMethod", "CallNonvirtualLongMethodV", "CallNonvirtualLongMethodA",
    "CallNonvirtualFloatMethod", "CallNonvirtualFloatMethodV", "CallNonvirtualFloatMethodA", "CallNonvirtualDoubleMethod",
    "CallNonvirtualDoubleMethodV", "CallNonvirtualDoubleMethodA", "CallNonvirtualVoidMethod", "CallNonvirtualVoidMethodV",
    "CallNonvirtualVoidMethodA", "GetFieldID", "GetObjectField", "GetBooleanField", "GetByteField", "GetCharField", "GetShortField",
    "GetIntField", "GetLongField", "GetFloatField", "GetDoubleField", "SetObjectField", "SetBooleanField", "SetByteField", "SetCharField",
    "SetShortField", "SetIntField", "SetLongField", "SetFloatField", "SetDoubleField", "GetStaticMethodID", "CallStaticObjectMethod",
    "CallStaticObjectMethodV", "CallStaticObjectMethodA", "CallStaticBooleanMethod", "CallStaticBooleanMethodV",
    "CallStaticBooleanMethodA", "CallStaticByteMethod", "CallStaticByteMethodV", "CallStaticByteMethodA", "CallStaticCharMethod",
    "CallStaticCharMethodV", "CallStaticCharMethodA", "CallStaticShortMethod", "CallStaticShortMethodV", "CallStaticShortMethodA",
    "CallStaticIntMethod", "CallStaticIntMethodV", "CallStaticIntMethodA", "CallStaticLongMethod", "CallStaticLongMethodV",
    "CallStaticLongMethodA", "CallStaticFloatMethod", "CallStaticFloatMethodV", "CallStaticFloatMethodA", "CallStaticDoubleMethod",
    "CallStaticDoubleMethodV", "CallStaticDoubleMethodA", "CallStaticVoidMethod", "CallStaticVoidMethodV", "CallStaticVoidMethodA",
    "GetStaticFieldID", "GetStaticObjectField", "GetStaticBooleanField", "GetStaticByteField", "GetStaticCharField", "GetStaticShortField",
    "GetStaticIntField", "GetStaticLongField", "GetStaticFloatField", "GetStaticDoubleField", "SetStaticObjectField",
    "SetStaticBooleanField", "SetStaticByteField", "SetStaticCharField", "SetStaticShortField", "SetStaticIntField", "SetStaticLongField",
    "SetStaticFloatField", "SetStaticDoubleField", "NewString", "GetStringLength", "GetStringChars", "ReleaseStringChars", "NewStringUTF",
    "GetStringUTFLength", "GetStringUTFChars", "ReleaseStringUTFChars", "GetArrayLength", "NewObjectArray", "GetObjectArrayElement",
    "SetObjectArrayElement", "NewBooleanArray", "NewByteArray", "NewCharArray", "NewShortArray", "NewIntArray", "NewLongArray",
    "NewFloatArray", "NewDoubleArray", "GetBooleanArrayElements", "GetByteArrayElements", "GetCharArrayElements", "GetShortArrayElements",
    "GetIntArrayElements", "GetLongArrayElements", "GetFloatArrayElements", "GetDoubleArrayElements", "ReleaseBooleanArrayElements",
    "ReleaseByteArrayElements", "ReleaseCharArrayElements", "ReleaseShortArrayElements", "ReleaseIntArrayElements",
    "ReleaseLongArrayElements", "ReleaseFloatArrayElements", "ReleaseDoubleArrayElements", "GetBooleanArrayRegion", "GetByteArrayRegion",
    "GetCharArrayRegion", "GetShortArrayRegion", "GetIntArrayRegion", "GetLongArrayRegion", "GetFloatArrayRegion", "GetDoubleArrayRegion",
    "SetBooleanArrayRegion", "SetByteArrayRegion", "SetCharArrayRegion", "SetShortArrayRegion", "SetIntArrayRegion", "SetLongArrayRegion",
    "SetFloatArrayRegion", "SetDoubleArrayRegion", "RegisterNatives", "UnregisterNatives", "MonitorEnter", "MonitorExit", "GetJavaVM",
    "GetStringRegion", "GetStringUTFRegion", "GetPrimitiveArrayCritical", "ReleasePrimitiveArrayCritical", "GetStringCritical",
    "ReleaseStringCritical", "NewWeakGlobalRef", "DeleteWeakGlobalRef", "ExceptionCheck", "NewDirectByteBuffer", "GetDirectBufferAddress",
    "GetDirectBufferCapacity", "GetObjectRefType", "GetModule",
];

/// Reserved slots before GetVersion.
pub const RESERVED: usize = 4;

/// The function a slot of the table holds.
pub fn function(slot: usize) -> Option<&'static str> {
    slot.checked_sub(RESERVED).and_then(|i| FUNCTIONS.get(i)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_runs_from_get_version_to_get_module() {
        assert_eq!(function(4), Some("GetVersion"));
        assert_eq!(function(21), Some("NewGlobalRef"));
        assert_eq!(function(24), Some("IsSameObject"));
        assert_eq!(function(233), Some("GetModule"));
        assert_eq!(function(3), None);
        assert_eq!(function(234), None);
    }
}
