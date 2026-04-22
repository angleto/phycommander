# Trademark policy

The project name **"PhyCommander"** and its short form **"PhyCMD"**,
together with the project logo (`physerver/static/logo.svg` and
`physerver/static/logo-mark.svg`) and any distinctive visual identity
associated with them, are unregistered trademarks of Angelo Leto.
They are **not** covered by the AGPL, GPL, CERN-OHL-S, or CC-BY-SA
licences that apply to the rest of the project.

You may:

  * **Refer to the project by name** in factual contexts:
    articles, papers, talks, forum posts, "I use PhyCommander in my
    lab", "our script targets the PhyCommander REST API", and so on.
    No permission required.

  * **Preserve attribution** in forks that stay close to upstream.
    If your fork tracks upstream and only adds patches you intend to
    submit back, you may keep the name and logo; this is the usual
    "downstream distro" case.

You may **not**:

  * **Rebrand and resell** the software without renaming. If you fork
    the project, make substantial changes, and distribute the result
    to third parties (commercially or otherwise), rename your fork.
    Do not present modified versions as "PhyCommander" or "PhyCMD".

  * **Use the logo** on packaging, marketing material, or products
    that are not this project or a compatible unmodified build of it.
    The logo identifies the original; using it on a derivative work
    misrepresents the source.

  * **Register** "PhyCommander", "PhyCMD", or confusingly similar
    marks in any jurisdiction.

  * **Imply endorsement** by the author or the project through use
    of the name or logo.

## Forks that diverge: renaming guidance

If you fork the project and take it in a direction that is not a
candidate for upstream merge — a hardware variant with a different
pinout, a different protocol, a commercial derivative — please pick
a different name. A suggested pattern:

  * `<YourOrg>-Bench` or `<YourOrg>-LabCtrl` for in-house variants.
  * `OpenBench-<something>` for community forks focused on a
    different hardware target.

Keeping the rename visible in the README, the binary name, and the
package metadata (Cargo.toml `name`, pyproject.toml `name`, Debian
package, etc.) is sufficient. You may still cite "forked from
PhyCommander by Angelo Leto" factually — that is attribution, which
is encouraged.

## Contact

For uses that do not fit the rules above, or to request explicit
permission, contact the author at `angelo@leto.blue`. Requests for
academic reuse, conference / teaching use, and quotation in books or
papers are normally granted without fuss; please ask for completeness.

---

This policy is inspired by, and largely compatible with, the Mozilla
and Rust Foundation trademark policies. It is not a licence grant:
any permission it implies is revocable at the author's discretion if
the use misrepresents the project.
