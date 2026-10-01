plugins {
    id("com.android.application")
}

android {
    namespace = "com.oppa.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.oppa.app"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
    }

    buildTypes {
        debug {}
    }

    sourceSets {
        // Single manifest of truth lives one level up (the aapt2
        // path uses the same file — no duplicated manifest).
        getByName("main") {
            manifest.srcFile("../../AndroidManifest.xml")
        }
    }
}

// Stage the cargo-built .so files into jniLibs before merge (the
// .so files are built by cargo for both ABIs, not the NDK — Gradle
// only packages here). Relative to this file's project dir
// (gradle/app -> ../.. is the crate root).
tasks.register<Copy>("stageNativeLibs") {
    from("../../target/x86_64-linux-android/debug/liboppa_android_app.so") {
        rename { "liboppa_android_app.so" }
    }
    into("src/main/jniLibs/x86_64")
}

tasks.register<Copy>("stageNativeLibsArm64") {
    from("../../target/aarch64-linux-android/debug/liboppa_android_app.so") {
        rename { "liboppa_android_app.so" }
    }
    into("src/main/jniLibs/arm64-v8a")
}

tasks.named("preBuild") {
    dependsOn("stageNativeLibs", "stageNativeLibsArm64")
}
