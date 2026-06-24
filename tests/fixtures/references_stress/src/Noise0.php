<?php

namespace App;

class Noise0
{
    private function compute(): int
    {
        return 0;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
