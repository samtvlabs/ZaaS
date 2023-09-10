app_repository:
  boto_ecr.present:
    - name: app_repository
    - region: us-east-1

zeth_repository:
  boto_ecr.present:
    - name: zeth_repository
    - region: us-east-1