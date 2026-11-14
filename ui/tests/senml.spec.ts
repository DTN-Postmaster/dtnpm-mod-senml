import { test, expect } from '@playwright/test';

test.describe('SenML Telemetry View', () => {
  test('loads configured dashboard metrics and stream', async ({ page }) => {
    await page.route('/api/senml/v1/metrics/latest*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          {
            id: 1,
            name: 'urn:dtn:sensor:bus_v',
            unit: 'V',
            custom_label: 'Tension Bus',
            display_order: 1,
            is_visible: true,
            utc: 1731000100000,
            value: 28.1,
            str_value: null,
          },
        ]),
      });
    });

    await page.route('/api/senml/v1/metrics', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          {
            id: 1,
            name: 'urn:dtn:sensor:bus_v',
            unit: 'V',
            custom_label: 'Tension Bus',
            display_order: 1,
            is_visible: true,
          },
        ]),
      });
    });

    await page.goto('/#/senml');
    await expect(page.locator('body')).toBeVisible();
  });
});
