.PHONY: init bootstrap doctor check check-backend check-orchestrator check-dashboard check-contracts check-schemas check-docs demo

init:
	@scripts/init_workspace.sh

bootstrap:
	@scripts/bootstrap_dev.sh

doctor:
	@scripts/doctor.sh

check:
	@scripts/check_all.sh

check-backend:
	@scripts/check_backend.sh

check-orchestrator:
	@scripts/check_orchestrator.sh

check-dashboard:
	@scripts/check_dashboard.sh

check-contracts:
	@scripts/check_contracts.sh

check-schemas:
	@scripts/check_schemas.sh

check-docs:
	@scripts/check_docs.sh

demo:
	@scripts/run_demo_local.sh
