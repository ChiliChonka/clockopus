#!/usr/bin/env python3
"""Offline render tests for the clockopus chart: what is created, and what must fail."""
from pathlib import Path
import subprocess
import unittest

import yaml

CHART = Path(__file__).resolve().parents[1]
FIXTURE = CHART / "ci/test-values.yaml"


def render(*args, fixture=True):
    command = ["helm", "template", "clockopus", str(CHART), "-n", "time"]
    if fixture:
        command += ["-f", str(FIXTURE)]
    return subprocess.run(command + list(args), text=True, capture_output=True)


def docs_of(result):
    if result.returncode:
        raise AssertionError(result.stderr)
    return [d for d in yaml.safe_load_all(result.stdout) if d]


def env_of(container):
    out = {}
    for e in container.get("env", []):
        if "value" in e:
            out[e["name"]] = e["value"]
        else:
            ref = e["valueFrom"]["secretKeyRef"]
            out[e["name"]] = ("secret", ref["name"], ref["key"])
    return out


class FullInstallation(unittest.TestCase):
    """OIDC + PostgreSQL via CloudNativePG with backup, route and policies (the fixture)."""

    @classmethod
    def setUpClass(cls):
        cls.docs = docs_of(render())

    def one(self, kind, name=None):
        found = [d for d in self.docs if d["kind"] == kind and (name is None or d["metadata"]["name"] == name)]
        self.assertEqual(len(found), 1, (kind, name, [d["metadata"]["name"] for d in self.docs if d["kind"] == kind]))
        return found[0]

    def container(self):
        return self.one("Deployment")["spec"]["template"]["spec"]["containers"][0]

    def test_image_follows_the_app_version(self):
        chart = yaml.safe_load(open(CHART / "Chart.yaml"))
        self.assertEqual(chart["version"], chart["appVersion"], "released together from one tag")
        self.assertEqual(self.container()["image"], f"ghcr.io/chilichonka/clockopus:{chart['appVersion']}")

    def test_environment_for_oidc_and_postgres(self):
        env = env_of(self.container())
        self.assertEqual(env["CLOCKOPUS_AUTH"], "oidc")
        self.assertEqual(env["CLOCKOPUS_OIDC_ISSUER"], "https://auth.example.com/realms/family")
        self.assertEqual(env["CLOCKOPUS_OIDC_CLIENT_ID"], "clockopus")
        self.assertEqual(env["CLOCKOPUS_OIDC_AUDIENCE"], "clockopus", "audience defaults to the client")
        self.assertEqual(env["CLOCKOPUS_STORAGE"], "postgres")
        self.assertEqual(env["DATABASE_URL"], ("secret", "clockopus-db-app", "uri"), "CNPG app secret")
        self.assertEqual(env["CLOCKOPUS_LISTEN"], "0.0.0.0:8080", "IPv4: nodes without IPv6")
        self.assertNotIn("CLOCKOPUS_DATA_DIR", env)

    def test_hardened_pod(self):
        spec = self.one("Deployment")["spec"]["template"]["spec"]
        self.assertIs(spec["automountServiceAccountToken"], False)
        self.assertIs(spec["securityContext"]["runAsNonRoot"], True)
        sc = self.container()["securityContext"]
        self.assertIs(sc["readOnlyRootFilesystem"], True)
        self.assertIs(sc["allowPrivilegeEscalation"], False)
        self.assertEqual(sc["capabilities"]["drop"], ["ALL"])

    def test_probes(self):
        c = self.container()
        self.assertEqual(c["readinessProbe"]["httpGet"]["path"], "/readyz")
        self.assertEqual(c["livenessProbe"]["httpGet"]["path"], "/healthz")
        self.assertEqual(c["startupProbe"]["httpGet"]["path"], "/healthz")

    def test_two_replicas_with_postgres_get_a_pdb_and_rolling_updates(self):
        dep = self.one("Deployment")
        self.assertEqual(dep["spec"]["replicas"], 2)
        self.assertNotIn("strategy", dep["spec"])
        self.assertEqual(self.one("PodDisruptionBudget")["spec"]["minAvailable"], 1)

    def test_cnpg_cluster_and_backup(self):
        cluster = self.one("Cluster", "clockopus-db")
        self.assertEqual(cluster["spec"]["bootstrap"]["initdb"], {"database": "clockopus", "owner": "clockopus"})
        self.assertIs(cluster["spec"]["enableSuperuserAccess"], False)
        self.assertEqual(cluster["spec"]["plugins"][0]["parameters"]["barmanObjectName"], "clockopus-db-backup-store")
        store = self.one("ObjectStore", "clockopus-db-backup-store")
        self.assertEqual(store["spec"]["configuration"]["s3Credentials"]["region"]["key"], "region")
        self.assertEqual(self.one("ScheduledBackup")["spec"]["cluster"]["name"], "clockopus-db")

    def test_route_needs_section_and_points_to_the_service(self):
        route = self.one("HTTPRoute")
        self.assertEqual(route["spec"]["parentRefs"][0]["sectionName"], "https")
        self.assertEqual(route["spec"]["hostnames"], ["clockopus.example.com"])
        backend = route["spec"]["rules"][0]["backendRefs"][0]
        svc = self.one("Service")
        self.assertEqual((backend["name"], backend["port"]), (svc["metadata"]["name"], svc["spec"]["ports"][0]["port"]))

    def test_network_policies(self):
        app = self.one("NetworkPolicy", "clockopus-app")
        sources = [s.get("namespaceSelector", {}).get("matchLabels", {}).get("kubernetes.io/metadata.name")
                   for s in app["spec"]["ingress"][0]["from"]]
        self.assertIn("example-gateway", sources)
        db = self.one("NetworkPolicy", "clockopus-db")
        self.assertEqual(db["spec"]["podSelector"]["matchLabels"]["cnpg.io/cluster"], "clockopus-db")
        self.assertEqual(len(db["spec"]["ingress"]), 2, "app/operator + metrics")

    def test_no_pvc_with_postgres(self):
        self.assertEqual([d for d in self.docs if d["kind"] == "PersistentVolumeClaim"], [])


class YamlInstallation(unittest.TestCase):
    def test_one_replica_with_volume_kept_on_uninstall(self):
        docs = docs_of(render("--set", "auth.mode=none", "--set", "storage.type=yaml", fixture=False))
        dep = next(d for d in docs if d["kind"] == "Deployment")
        self.assertEqual(dep["spec"]["strategy"]["type"], "Recreate")
        env = env_of(dep["spec"]["template"]["spec"]["containers"][0])
        self.assertEqual((env["CLOCKOPUS_STORAGE"], env["CLOCKOPUS_DATA_DIR"]), ("yaml", "/data"))
        self.assertNotIn("DATABASE_URL", env)
        pvc = next(d for d in docs if d["kind"] == "PersistentVolumeClaim")
        self.assertEqual(pvc["metadata"]["annotations"]["helm.sh/resource-policy"], "keep")
        mounts = dep["spec"]["template"]["spec"]["containers"][0]["volumeMounts"]
        self.assertEqual(mounts[0]["mountPath"], "/data")
        kinds = sorted(d["kind"] for d in docs)
        self.assertEqual(kinds, ["Deployment", "PersistentVolumeClaim", "Service"])

    def test_existing_database_secret_without_cnpg(self):
        docs = docs_of(render(
            "--set", "auth.issuer=https://a", "--set", "auth.clientId=c",
            "--set", "storage.postgres.existingSecret=my-db", "--set", "storage.postgres.uriKey=url",
            fixture=False,
        ))
        dep = next(d for d in docs if d["kind"] == "Deployment")
        env = env_of(dep["spec"]["template"]["spec"]["containers"][0])
        self.assertEqual(env["DATABASE_URL"], ("secret", "my-db", "url"))
        self.assertFalse([d for d in docs if d["kind"] == "Cluster"])


class RefusedCombinations(unittest.TestCase):
    CASES = [
        (["--set", "auth.issuer="], "needs auth.issuer and auth.clientId"),
        (["--set", "auth.mode=none"], "auth.mode=none with httpRoute.enabled"),
        (["--set", "auth.mode=basic"], "auth.mode must be oidc or none"),
        (["--set", "storage.type=yaml", "--set", "cnpg.enabled=false", "--set", "cnpg.backup.enabled=false"],
         "storage.type=yaml supports exactly one replica"),
        (["--set", "cnpg.enabled=false", "--set", "cnpg.backup.enabled=false"],
         "needs cnpg.enabled or storage.postgres.existingSecret"),
        (["--set", "cnpg.image="], "cnpg.image is required"),
        (["--set", "httpRoute.parentRef.sectionName="], "sectionName is required"),
        (["--set", "cnpg.enabled=false", "--set", "storage.postgres.existingSecret=db"],
         "cnpg.backup.enabled requires cnpg.enabled"),
    ]

    def test_each_combination_fails_with_its_message(self):
        failed = 0
        for args, message in self.CASES:
            with self.subTest(args=args):
                result = render(*args)
                self.assertNotEqual(result.returncode, 0, args)
                self.assertIn(message, result.stderr)
                failed += 1
        self.assertEqual(failed, len(self.CASES))
        # Counter-check: the fixture itself renders.
        self.assertEqual(render().returncode, 0)


if __name__ == "__main__":
    unittest.main()
