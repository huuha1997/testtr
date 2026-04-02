# Self-Hosted Agentic Design-to-Code

## Mục tiêu
- Tự host pipeline từ prompt đến frontend production-ready.
- Bắt buộc có bước user chọn mockup và chọn tech stack qua UI chat.
- Chuẩn hóa mọi bước bằng contract để giảm lệch đầu ra.

## Scope MVP
- Kết nối provider: Banana, Stitch, Claude Code, GitHub, Vercel.
- Sinh 3 mockup A/B/C, cho phép user chọn hoặc merge.
- Khóa Generation Contract sau khi user chốt mockup + stack.
- Tự động codegen, chạy quality gates, tự sửa lỗi trong ngưỡng.
- Tạo PR, deploy preview, approve rồi deploy production.

## Non-Goals MVP
- Không sinh backend domain phức tạp.
- Không triển khai multi-region.
- Không hỗ trợ collaborative editing realtime.

## Nguyên tắc
- Deterministic theo contract đã khóa.
- Mọi transition phải audit được.
- Không deploy production nếu fail bất kỳ gate bắt buộc nào.
- Token provider luôn được mã hóa và giới hạn scope.
