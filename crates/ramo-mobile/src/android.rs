use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JObject};
use jni::sys::{JNI_TRUE, jboolean};

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_carlosarraes_ramo_network_NativeNetworkBootstrap_initializeNative<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) -> jboolean {
    unowned_env
        .with_env(|env| {
            rustls_platform_verifier::android::init_with_env(env, context)?;
            Ok::<jboolean, jni::errors::Error>(JNI_TRUE)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
