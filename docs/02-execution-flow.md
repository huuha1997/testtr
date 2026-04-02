# Execution Flow

## End-to-End
1. User nhập brief trong chat.
2. Hệ thống kiểm tra provider connections bắt buộc.
3. Banana tạo 3 mockup A/B/C.
4. User chọn mockup hoặc merge rules.
5. User chọn stack preset và tùy chọn.
6. Hệ thống khóa `Generation Contract v1`.
7. Stitch trích xuất tokens + component tree + interaction spec.
8. Claude Code tạo code theo contract.
9. CI chạy quality gates.
10. Nếu fail thì self-heal theo taxonomy trong giới hạn vòng lặp.
11. Pass gates thì tạo branch + PR + preview URL.
12. User approve để deploy production.

## Quality Gates
- Lint pass
- Typecheck pass
- Build pass
- Playwright smoke pass
- Visual diff dưới ngưỡng
- A11y critical bằng 0

## Stop Conditions
- Vượt `max_repair_iterations`
- Provider timeout/retry vượt policy
- Thiếu connection scope bắt buộc
- User hủy run
