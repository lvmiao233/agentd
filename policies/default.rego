package agentd.policy

import rego.v1

# Repository default policy module.
#
# Keep this module side-effect free by default so layered TOML/transpiled
# policy behavior remains the primary decision source.
#
# Operators can extend this file (or add extra *.rego modules under policies/)
# to enforce global deny/allow constraints across all agents.

default allow := false
default deny := false
