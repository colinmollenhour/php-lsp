<?php

namespace App;

class Noise5
{
    private function compute(): int
    {
        return 5;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
