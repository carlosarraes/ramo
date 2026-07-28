import groovy.json.JsonSlurper

@Suppress("UNCHECKED_CAST")
fun rustlsVerifierMavenDirectory(): File {
    val metadata = providers.exec {
        workingDir = File(settingsDir, "..")
        commandLine(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-linux-android",
        )
    }.standardOutput.asText.get()
    val root = JsonSlurper().parseText(metadata) as Map<String, Any?>
    val packages = root.getValue("packages") as List<Map<String, Any?>>
    val manifestPath = packages
        .singleOrNull { it["name"] == "rustls-platform-verifier-android" }
        ?.get("manifest_path") as? String
        ?: error("Cargo metadata does not contain rustls-platform-verifier-android")
    return File(File(manifestPath).parentFile, "maven")
}

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri(rustlsVerifierMavenDirectory())
            metadataSources { artifact() }
        }
    }
}

rootProject.name = "Ramo"
include(":app")
