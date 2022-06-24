/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import com.moandjiezana.toml.TomlWriter
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.DependencyScope
import software.amazon.smithy.rust.codegen.rustlang.Feature
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.smithy.CoreRustSettings
import software.amazon.smithy.rust.codegen.util.deepMergeWith

/**
 * Customizations to apply to the generated Cargo.toml file.
 *
 * This is a nested map of key/value that represents the properties in a crate manifest.
 * For example, the following
 *
 * ```kotlin
 * mapOf(
 *     "package" to mapOf(
 *         "name" to "foo",
 *         "version" to "1.0.0",
 *     )
 * )
 * ```
 *
 * is equivalent to
 *
 * ```toml
 * [package]
 * name = "foo"
 * version = "1.0.0"
 * ```
 */
typealias ManifestCustomizations = Map<String, Any?>

/**
 * Generates the crate manifest Cargo.toml file.
 */
class CargoTomlGenerator(
    private val settings: CoreRustSettings,
    private val writer: RustWriter,
    private val manifestCustomizations: ManifestCustomizations,
    private val dependencies: List<CargoDependency>,
    private val features: List<Feature>
) {
    fun render() {
        val cargoFeatures = features.map { it.name to it.deps }.toMutableList()
        if (features.isNotEmpty()) {
            cargoFeatures.add("default" to features.filter { it.default }.map { it.name })
        }

        val cargoToml = mapOf(
            "package" to listOfNotNull(
                "name" to settings.moduleName,
                "version" to settings.moduleVersion,
                "authors" to settings.moduleAuthors,
                settings.moduleDescription?.let { "description" to it },
                "edition" to "2021",
                "license" to settings.license,
                "repository" to settings.moduleRepository,
            ).toMap(),
            "dependencies" to dependencies.filter { it.scope == DependencyScope.Compile }
                .associate { it.name to it.toMap() },
            "dev-dependencies" to dependencies.filter { it.scope == DependencyScope.Dev }
                .associate { it.name to it.toMap() },
            "features" to cargoFeatures.toMap()
        ).deepMergeWith(manifestCustomizations)

        writer.writeWithNoFormatting(TomlWriter().write(cargoToml))
    }
}
