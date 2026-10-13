plugins {
    id("com.android.application")
}

// The version comes from package.sh (the workspace version, and a code that grows with the build
// time so a newer build installs over an older one).
val wcVersionName = providers.environmentVariable("WORDCRAFT_VERSION_NAME").orElse("0.0.0-dev")
val wcVersionCode = providers.environmentVariable("WORDCRAFT_VERSION_CODE").map { it.toInt() }.orElse(1)

// Release signing material lives outside the repository; see packaging/android/README.md.
val keystorePath = providers.environmentVariable("WORDCRAFT_KEYSTORE").orNull
val keystorePassword = providers.environmentVariable("WORDCRAFT_KEYSTORE_PASSWORD").orNull

android {
    namespace = "ai.storyteller.wordcraft"
    compileSdk = 37
    buildToolsVersion = "36.1.0"
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "ai.storyteller.wordcraft"
        // Android 15 and up: the app is built for a Galaxy Z Fold7 (Android 16, 17).
        minSdk = 35
        // 36: the window is edge to edge (enforced from 35); MainActivity applies the insets as padding.
        targetSdk = 36
        versionCode = wcVersionCode.get()
        versionName = wcVersionName.get()
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        if (keystorePath != null && keystorePassword != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = keystorePassword
                keyAlias = providers.environmentVariable("WORDCRAFT_KEY_ALIAS").getOrElse("wordcraft")
                keyPassword = providers.environmentVariable("WORDCRAFT_KEY_PASSWORD").getOrElse(keystorePassword)
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
                enableV4Signing = false
            }
        }
    }

    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            isShrinkResources = false
            signingConfig = signingConfigs.findByName("release")
            // No build information about the machine or the repository inside the APK.
            vcsInfo.include = false
        }
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.core:core:1.17.0")
    implementation("androidx.games:games-activity:4.4.0")
}
