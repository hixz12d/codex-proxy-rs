"""Mocked browser checks for account egress. Requires Python Playwright and Chromium.
Run against the local Vite server; no production services or credentials are used.
"""
import argparse
import json
from pathlib import Path
from urllib.parse import urlparse
from playwright.sync_api import sync_playwright, expect


def account(identifier, name):
    usage = {"requestCount": 0, "requestCountDisplay": "0", "costs": [], "models": [], "lastUsedAt": None, "lastUsedAtDisplay": "-"}
    return {"id": identifier, "name": name, "provider": "openai", "resourceRef": identifier,
            "email": name.lower().replace(" ", ".") + "@example.invalid", "accountId": None,
            "userId": None, "label": None, "planType": "plus", "authenticationKind": "oauth",
            "hasRefreshToken": True, "status": "normal", "errorReason": None, "errorMessage": None,
            "enabled": True, "concurrencyLimit": None, "weight": 1, "accessTokenExpiresAt": None,
            "accessTokenExpiresAtDisplay": "-", "refreshTokenExpiresAt": None, "nextRefreshAt": None,
            "addedAt": "2026-09-10T00:00:00Z", "addedAtDisplay": "2026-09-10", "updatedAt": "2026-09-10T00:00:00Z",
            "updatedAtDisplay": "2026-09-10", "groups": [], "usage": usage,
            "quota": {"windows": [], "refreshedAtDisplay": "-", "limitReached": False, "rateLimitedUntil": None}}


def verify(browser, url, output, width, height):
    context = browser.new_context(viewport={"width": width, "height": height})
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    state = {"proxies": [{"id": "proxy_one", "name": "US West", "address": "socks5h://proxy.example.invalid:1080", "enabled": True, "revision": 1, "accountCount": 1}],
             "bindings": {"acct_one": "proxy_one"}, "mutations": [], "fail_next": False}
    accounts = [account("acct_one", "Test One"), account("acct_two", "Test Two")]

    def route(request):
        path = urlparse(request.request.url).path
        status = 200
        data = {}
        if path.endswith("/auth/status"):
            data = {"authenticated": True}
        elif path.endswith("/accounts"):
            data = {"items": accounts, "page": {"page": 1, "pageSize": 20, "total": 2, "totalPages": 1},
                    "summary": {"total": 2, "normal": 2, "quotaExhausted": 0, "rateLimited": 0, "disabled": 0, "error": 0}}
        elif path.endswith("/account-groups"):
            data = {"items": [], "page": {"page": 1, "pageSize": 50, "total": 0, "totalPages": 0}, "configRevision": 1}
        elif path.endswith("/egress"):
            if state["fail_next"]:
                state["fail_next"] = False
                status = 503
            else:
                data = {"proxies": state["proxies"], "bindings": state["bindings"]}
        elif path.endswith("/egress/update"):
            mutation = request.request.post_data_json
            state["mutations"].append(mutation)
            action = mutation["action"]
            if action == "save":
                if mutation.get("id"):
                    node = next(p for p in state["proxies"] if p["id"] == mutation["id"])
                    assert node["revision"] == mutation["expectedRevision"]
                    node.update(name=mutation["name"], enabled=mutation["enabled"], revision=node["revision"] + 1)
                else:
                    assert mutation["endpoint"].startswith("socks5h://")
                    state["proxies"].append({"id": "proxy_new", "name": mutation["name"], "enabled": mutation["enabled"], "revision": 1, "accountCount": 0, "address": "socks5h://new.example.invalid:1080"})
            elif action == "bind":
                for identifier in mutation["accountIds"]:
                    if mutation["proxyId"]:
                        state["bindings"][identifier] = mutation["proxyId"]
                    else:
                        state["bindings"].pop(identifier, None)
                for node in state["proxies"]:
                    node["accountCount"] = sum(value == node["id"] for value in state["bindings"].values())
            elif action == "delete":
                state["proxies"] = [p for p in state["proxies"] if p["id"] != mutation["id"]]
            data = {"configRevision": len(state["mutations"]) + 1}
        request.fulfill(status=status, content_type="application/json", body=json.dumps({"code": 0 if status == 200 else 1, "message": "OK" if status == 200 else "Unavailable", "data": data}))

    page.route("**/api/**", route)
    page.goto(url + "/accounts")
    page.get_by_role("button", name="出口代理", exact=True).click()
    dialog = page.get_by_role("dialog")
    expect(dialog.get_by_text("US West", exact=True)).to_be_visible()
    expect(dialog.get_by_role("button", name="节点仍有绑定账号")).to_be_disabled()
    page.wait_for_timeout(500)
    page.screenshot(path=str(output / f"egress-manager-{width}.png"), full_page=True)
    dialog.get_by_role("button", name="新增代理", exact=True).click()
    dialog.get_by_label("节点名称", exact=True).fill("New Exit")
    dialog.get_by_label("代理地址和认证", exact=True).fill("socks5h://test:secret-not-displayed@new.example.invalid:1080")
    expect(dialog.get_by_label("代理地址和认证", exact=True)).to_have_attribute("type", "password")
    dialog.get_by_role("button", name="保存节点", exact=True).click()
    expect(dialog.get_by_text("New Exit", exact=True)).to_be_visible()
    assert "secret-not-displayed" not in dialog.inner_text()
    dialog.get_by_role("button", name="编辑 New Exit", exact=True).click()
    expect(dialog.get_by_label("代理地址和认证", exact=True)).to_have_value("")
    dialog.get_by_label("节点名称", exact=True).fill("Renamed Exit")
    dialog.get_by_role("button", name="保存节点", exact=True).click()
    expect(dialog.get_by_text("Renamed Exit", exact=True)).to_be_visible()
    assert "endpoint" not in state["mutations"][-1]
    dialog.get_by_role("button", name="删除 Renamed Exit", exact=True).click()
    dialog.get_by_role("button", name="确认删除", exact=True).click()
    expect(dialog.get_by_text("Renamed Exit", exact=True)).to_have_count(0)
    dialog.get_by_role("button", name="关闭", exact=True).last.click()
    page.get_by_role("button", name="绑定 Test Two 出口", exact=True).click()
    dialog.get_by_role("combobox", name="账号出口", exact=True).click()
    page.get_by_role("option", name="US West", exact=True).click()
    box = dialog.bounding_box()
    assert box and box["x"] >= -1 and box["x"] + box["width"] <= width + 1
    page.wait_for_timeout(500)
    page.screenshot(path=str(output / f"egress-binding-{width}.png"), full_page=True)
    dialog.get_by_role("button", name="保存出口", exact=True).click()
    expect(dialog).to_have_count(0)
    assert state["bindings"]["acct_two"] == "proxy_one"
    page.get_by_role("checkbox").first.press("Space")
    page.get_by_role("button", name="绑定出口", exact=True).click()
    dialog.get_by_role("combobox", name="账号出口", exact=True).click()
    page.get_by_role("option", name="直连", exact=True).click()
    dialog.get_by_role("button", name="保存出口", exact=True).click()
    expect(dialog).to_have_count(0)
    assert not state["bindings"]
    assert set(state["mutations"][-1]["accountIds"]) == {"acct_one", "acct_two"}
    state["fail_next"] = True
    page.get_by_role("button", name="出口代理", exact=True).click()
    expect(dialog.get_by_role("alert")).to_contain_text("代理配置加载失败")
    dialog.get_by_role("button", name="重新加载代理配置", exact=True).click()
    expect(dialog.get_by_text("US West", exact=True)).to_be_visible()
    assert not errors, errors
    context.close()
    return {"viewport": [width, height], "mutations": len(state["mutations"]), "pageErrors": errors}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:5187")
    parser.add_argument("--channel", default=None, help="Installed Chromium channel, e.g. msedge or chrome")
    parser.add_argument("--output", default=".runtime/egress-ui")
    args = parser.parse_args()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.launch(channel=args.channel)
        results = [verify(browser, args.url, output, *size) for size in [(1440, 1000), (390, 844)]]
        browser.close()
    print(json.dumps(results, ensure_ascii=False))


if __name__ == "__main__":
    main()
