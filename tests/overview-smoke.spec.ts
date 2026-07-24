if (process.env.VITEST) {
  const { test } = await import('vitest');
  test.skip('playwright smoke runs only under Playwright', () => {});
} else {
  const { expect, test } = await import('@playwright/test');

  test('covers live, unavailable, and explicit demo states', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByRole('button', { name: 'Live' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(page.getByLabel('Telemetry state: Unavailable').first()).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Detailed turn telemetry is not configured.' }),
    ).toBeVisible();
    await expect(page.getByLabel(/Open alert center, 0 active alerts/i).first()).toBeVisible();

    await page.getByRole('button', { name: 'Demo', exact: true }).click();
    await expect(page.getByText('DEMO DATA').first()).toBeVisible();
    await expect(page.getByText(/Nothing here is persisted/i)).toBeVisible();
    await expect(page.getByLabel(/Warning quota alert for Codex 5 hour window/i)).toBeVisible();
    await expect(
      page.getByLabel(/Critical quota alert for Synthetic critical example/i),
    ).toBeVisible();
    await expect(page.getByLabel(/Open alert center, 2 active alerts/i).first()).toBeVisible();

    await expect(page.getByRole('heading', { name: 'Source health' })).toBeVisible();
    await expect(page.getByText(/Last success/i).first()).toBeVisible();
    await expect(page.getByText(/Supplies: Quota windows, Account activity/i)).toBeVisible();

    await page
      .getByRole('navigation', { name: 'Primary navigation' })
      .getByRole('button', { name: /Alerts/ })
      .click();
    await expect(page.getByRole('heading', { name: 'Alert center' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Active alerts' })).toBeVisible();

    await page.getByRole('button', { name: 'Usage Burn' }).click();
    await expect(page.getByText(/Correlation only/i)).toBeVisible();
    await expect(page.getByText('Exact quota units')).toBeVisible();

    await page.getByRole('button', { name: 'Live', exact: true }).click();
    await expect(page.getByText('DEMO DATA')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Live' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });
}
