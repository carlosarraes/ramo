import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val repositoryRoot = rootProject.projectDir.parentFile
val generatedUniFfi = projectDir.resolve("build/generated/source/uniffi/main")
val generatedJniLibs = projectDir.resolve("build/generated/jniLibs/main")
val localProperties = Properties().apply {
    rootProject.file("local.properties").inputStream().use(::load)
}
val androidSdk = requireNotNull(localProperties.getProperty("sdk.dir")) {
    "sdk.dir must be set in android/local.properties; run scripts/bootstrap-android.sh"
}
val androidNdk = file("$androidSdk/ndk/28.2.13676358")

val buildRustAndroid by tasks.registering(Exec::class) {
    workingDir(repositoryRoot)
    environment("ANDROID_NDK_HOME", androidNdk.absolutePath)
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a",
        "-o", "android/app/build/generated/jniLibs/main",
        "build", "--release", "-p", "ramo-mobile",
    )
}

val buildRustHost by tasks.registering(Exec::class) {
    workingDir(repositoryRoot)
    commandLine("cargo", "build", "-p", "ramo-mobile")
}

val generateUniFfiAndroid by tasks.registering(Exec::class) {
    dependsOn(buildRustAndroid)
    workingDir(repositoryRoot)
    commandLine(
        "cargo", "run", "-p", "uniffi-bindgen", "--",
        "generate", "--library",
        "target/aarch64-linux-android/release/libramo_mobile.so",
        "--language", "kotlin", "--out-dir",
        "android/app/build/generated/source/uniffi/main",
    )
}

android {
    namespace = "io.github.carlosarraes.ramo"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "io.github.carlosarraes.ramo"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        buildConfigField("String", "APP_NAME", "\"Ramo\"")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += "arm64-v8a" }
    }

    buildFeatures {
        buildConfig = true
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    val keystorePropertiesFile = rootProject.file("keystore.properties")
    val releaseSigning = if (keystorePropertiesFile.exists()) {
        val signingProperties = Properties().apply { keystorePropertiesFile.inputStream().use(::load) }
        signingConfigs.create("personalRelease") {
            storeFile = file(requireNotNull(signingProperties.getProperty("storeFile")))
            storePassword = requireNotNull(signingProperties.getProperty("storePassword"))
            keyAlias = requireNotNull(signingProperties.getProperty("keyAlias"))
            keyPassword = requireNotNull(signingProperties.getProperty("keyPassword"))
        }
    } else null

    buildTypes.getByName("release") {
        isMinifyEnabled = false
        signingConfig = releaseSigning
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    sourceSets.getByName("main") {
        kotlin.directories.add(generatedUniFfi.absolutePath)
        jniLibs.directories.add(generatedJniLibs.absolutePath)
    }
}

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.work.runtime)
    implementation("net.java.dev.jna:jna:5.12.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    debugImplementation(libs.androidx.compose.ui.tooling)
    testImplementation(kotlin("test-junit"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testRuntimeOnly("net.java.dev.jna:jna:5.12.0")
    androidTestImplementation("androidx.test:core-ktx:1.7.0")
    androidTestImplementation("androidx.test.ext:junit-ktx:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
}

tasks.matching { it.name.startsWith("compile") && it.name.endsWith("Kotlin") }.configureEach {
    dependsOn(generateUniFfiAndroid)
}

tasks.matching { it.name == "testDebugUnitTest" }.configureEach {
    dependsOn(buildRustHost)
    (this as Test).systemProperty(
        "jna.library.path",
        repositoryRoot.resolve("target/debug").absolutePath,
    )
}
