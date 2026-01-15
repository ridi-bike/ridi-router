# Test Data

This directory contains test data for end-to-end testing of the RMDF tile system.

## Montenegro PBF

The Montenegro PBF file is required for E2E tests but not included in the repository due to size.

**Download:**
```bash
wget -O test_data/montenegro.osm.pbf \
  https://download.geofabrik.de/europe/montenegro-latest.osm.pbf
```

**Source:** https://download.geofabrik.de/europe/montenegro.html

**Approximate size:** ~10MB

**Last updated:** Check Geofabrik for latest version

## Test Outputs

The E2E tests will create the following directories:
- `montenegro_tiles_e2e/` - Generated RMDF tiles for testing
- `tiles_backup/` - Temporary backup during missing tile tests

These directories are automatically cleaned up by the tests.
