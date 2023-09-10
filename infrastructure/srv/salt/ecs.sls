cluster:
  boto_ecs.cluster_present:
    - name: cluster
    - region: us-east-1

app_service:
  boto_ecs.service_present:
    - name: appService
    - cluster: cluster
    - desired_count: 2
    - task_definition: task_definition
    - region: us-east-1