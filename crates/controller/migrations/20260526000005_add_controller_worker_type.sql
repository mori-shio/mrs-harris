-- worker_definitions, workers, dag_tasks テーブルの worker_type ENUM に 'controller' を追加

ALTER TABLE worker_definitions 
MODIFY COLUMN worker_type ENUM('fargate', 'lambda', 'controller') NOT NULL DEFAULT 'fargate';

ALTER TABLE workers 
MODIFY COLUMN worker_type ENUM('fargate', 'lambda', 'controller') NOT NULL;

ALTER TABLE dag_tasks 
MODIFY COLUMN worker_type ENUM('fargate', 'lambda', 'controller') NOT NULL DEFAULT 'fargate';
