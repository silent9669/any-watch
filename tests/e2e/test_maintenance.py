import contextlib
import socket
import subprocess
import time

import pytest
from playwright.sync_api import expect, sync_playwright


VIEWPORTS = [
    (320, 568),
    (375, 812),
    (390, 844),
    (430, 932),
    (768, 1024),
    (1024, 768),
    (1280, 800),
    (1440, 900),
    (1728, 1117),
]


@pytest.fixture(scope="module")
def maintenance_url():
    proc = subprocess.Popen(
        ["python3", "-m", "http.server", "4174", "--directory", "maintenance", "--bind", "127.0.0.1"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(30):
            with contextlib.suppress(OSError):
                with socket.create_connection(("127.0.0.1", 4174), timeout=0.25):
                    break
            time.sleep(0.1)
        else:
            raise RuntimeError("maintenance preview server did not start")
        yield "http://127.0.0.1:4174"
    finally:
        proc.terminate()
        proc.wait(timeout=5)


@pytest.mark.parametrize("width,height", VIEWPORTS)
def test_exact_viewport_without_overflow(maintenance_url, width, height):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page(viewport={"width": width, "height": height})
        page.goto(maintenance_url, wait_until="networkidle")
        metrics = page.evaluate(
            """() => ({
                innerWidth,
                innerHeight,
                scrollWidth: document.documentElement.scrollWidth,
                scrollHeight: document.documentElement.scrollHeight,
                bodyWidth: document.body.scrollWidth,
                bodyHeight: document.body.scrollHeight,
                headline: document.querySelector('#maintenance-title').getBoundingClientRect().toJSON(),
                button: document.querySelector('#check-again').getBoundingClientRect().toJSON(),
                poster: document.querySelector('.maintenance-frame').getBoundingClientRect().toJSON(),
            })"""
        )
        assert metrics["scrollWidth"] == metrics["innerWidth"]
        assert metrics["bodyWidth"] == metrics["innerWidth"]
        assert metrics["scrollHeight"] <= metrics["innerHeight"]
        assert metrics["bodyHeight"] <= metrics["innerHeight"]
        assert metrics["headline"]["top"] >= 0
        assert metrics["headline"]["bottom"] <= height
        assert metrics["button"]["height"] >= 44
        assert metrics["button"]["bottom"] <= height
        assert metrics["poster"]["left"] >= 0
        assert metrics["poster"]["right"] <= width
        browser.close()


def test_keyboard_focus_and_reduced_motion(maintenance_url):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        context = browser.new_context(reduced_motion="reduce")
        page = context.new_page()
        page.goto(maintenance_url, wait_until="networkidle")
        page.keyboard.press("Tab")
        expect(page.locator(".announcement__track")).to_be_focused()
        page.keyboard.press("Tab")
        expect(page.locator("#check-again")).to_be_focused()
        values = page.evaluate(
            """() => ({
                ticker: getComputedStyle(document.querySelector('.announcement__track')).animationDuration,
                bulbs: getComputedStyle(document.querySelector('.bulb-rail')).animationDuration,
                focus: getComputedStyle(document.querySelector('#check-again')).outlineStyle,
            })"""
        )
        assert float(values["ticker"].replace("s", "")) <= 0.001
        assert float(values["bulbs"].replace("s", "")) <= 0.001
        assert values["focus"] != "none"
        browser.close()


def test_bundled_fonts_cover_vietnamese(maintenance_url):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page()
        page.goto(maintenance_url, wait_until="networkidle")
        coverage = page.evaluate(
            """async () => {
                const display = await document.fonts.load(
                    '700 32px "Barlow Condensed"',
                    'Rạp gia đình đang bảo trì'
                );
                const body = await document.fonts.load(
                    '400 16px Manrope',
                    'Lịch sử xem của bạn vẫn an toàn'
                );
                const mono = await document.fonts.load(
                    '500 12px "IBM Plex Mono"',
                    'SẼ TRỞ LẠI SỚM'
                );
                return {
                    display: display.length,
                    body: body.length,
                    mono: mono.length,
                };
            }"""
        )
        assert all(count > 0 for count in coverage.values())
        browser.close()


def test_key_contrast_ratios(maintenance_url):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page()
        page.goto(maintenance_url, wait_until="networkidle")
        ratios = page.evaluate(
            """() => {
                const pixel = (value) => {
                    const canvas = document.createElement('canvas');
                    canvas.width = canvas.height = 1;
                    const context = canvas.getContext('2d');
                    context.fillStyle = value;
                    context.fillRect(0, 0, 1, 1);
                    return [...context.getImageData(0, 0, 1, 1).data].slice(0, 3);
                };
                const luminance = (rgb) => rgb
                    .map((channel) => channel / 255)
                    .map((channel) => channel <= 0.04045
                        ? channel / 12.92
                        : ((channel + 0.055) / 1.055) ** 2.4)
                    .reduce((sum, channel, index) => sum + channel * [0.2126, 0.7152, 0.0722][index], 0);
                const ratio = (foreground, background) => {
                    const values = [luminance(pixel(foreground)), luminance(pixel(background))].sort((a, b) => b - a);
                    return (values[0] + 0.05) / (values[1] + 0.05);
                };
                const poster = getComputedStyle(document.querySelector('.maintenance-poster'));
                const title = getComputedStyle(document.querySelector('#maintenance-title'));
                const message = getComputedStyle(document.querySelector('#maintenance-message'));
                const button = getComputedStyle(document.querySelector('#check-again'));
                return {
                    title: ratio(title.color, poster.backgroundColor),
                    message: ratio(message.color, poster.backgroundColor),
                    button: ratio(button.color, button.backgroundColor),
                };
            }"""
        )
        assert ratios["title"] >= 4.5
        assert ratios["message"] >= 4.5
        assert ratios["button"] >= 4.5
        browser.close()


def test_refresh_online_offline_and_failure_states(maintenance_url):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        context = browser.new_context(locale="en-GB", timezone_id="UTC")
        page = context.new_page()
        mode = {"value": "maintenance"}

        def status_route(route):
            if mode["value"] == "failed":
                route.abort("failed")
                return
            route.fulfill(
                json={
                    "mode": mode["value"],
                    "headline": "The theatre is taking a short break.",
                    "message": "Your family library remains safe on the home server.",
                    "statusLabel": "The theatre is online" if mode["value"] == "online" else "Maintenance in progress",
                    "expectedReturn": "Shortly",
                    "detectedAtIso": "2026-08-13T10:15:42.000Z" if mode["value"] == "maintenance" else None,
                    "privacy": "Account data stays on your home server.",
                }
            )

        page.route("**/status.json?now=*", status_route)
        page.goto(maintenance_url, wait_until="networkidle")
        button = page.locator("#check-again")
        expect(page.locator("#down-detected")).to_have_text("13 Aug 2026 · 10:15:42 UTC")
        expect(page.locator("#down-detected")).to_have_attribute("datetime", "2026-08-13T10:15:42.000Z")
        expect(page).to_have_title("any-watch · Theatre maintenance")
        expect(page.locator(".brand-lockup strong")).to_have_text("any-watch")

        button.click()
        expect(button).to_have_attribute("data-state", "offline")
        expect(page.locator("#status-live")).to_contain_text("still offline")

        mode["value"] = "online"
        button.click()
        expect(button).to_have_attribute("data-state", "online")
        expect(page.locator("#status-live")).to_contain_text("back online")

        mode["value"] = "failed"
        button.click()
        expect(button).to_have_attribute("data-state", "failed")
        expect(page.locator("#status-live")).to_contain_text("request failed")
        browser.close()


def test_missing_detection_time_is_honest(maintenance_url):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page()
        page.route(
            "**/status.json?now=*",
            lambda route: route.fulfill(json={
                "mode": "maintenance",
                "headline": "The theatre is taking a short break.",
                "message": "any-watch is temporarily offline for maintenance.",
                "statusLabel": "Maintenance in progress",
                "expectedReturn": "Shortly",
                "detectedAtIso": None,
                "privacy": "Account data stays on your home server.",
            }),
        )
        page.goto(maintenance_url, wait_until="networkidle")
        expect(page.locator("#down-detected")).to_have_text("Detection time unavailable")
        assert page.locator("body").inner_text().lower().find("ani-desk") == -1
        browser.close()
